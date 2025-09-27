use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{RwLock, mpsc};
use wasmtime::{Engine, Instance, Linker, Module, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder};
use wasmtime_wasi::preview1::{WasiP1Ctx};
use anyhow::Result;
use tracing::{info, warn, error};

use super::api::{
    PluginManifest, PluginEvent, PluginCommand, PluginResponse,
    PluginContext, PluginHook, API_VERSION,
};
use crate::error::FerrixError;

/// WASM plugin runtime manager
pub struct PluginRuntime {
    engine: Engine,
    plugins: Arc<RwLock<HashMap<String, LoadedPlugin>>>,
    event_tx: mpsc::UnboundedSender<PluginEvent>,
    event_rx: Option<mpsc::UnboundedReceiver<PluginEvent>>,
    hook_registry: Arc<RwLock<HookRegistry>>,
}

struct LoadedPlugin {
    id: String,
    manifest: PluginManifest,
    instance: Instance,
    store: Arc<Mutex<Store<PluginState>>>,
    exports: HashMap<String, wasmtime::Func>,
}

struct PluginState {
    wasi: WasiP1Ctx,
    context: PluginContext,
    manifest: PluginManifest,
    event_queue: Vec<PluginEvent>,
}

// No WasiView implementation needed for Preview 1

struct HookRegistry {
    hooks: HashMap<PluginHook, Vec<String>>, // Hook -> Vec<plugin_id>
}

impl PluginRuntime {
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_simd(true);
        config.wasm_bulk_memory(true);
        config.async_support(true);

        let engine = Engine::new(&config)?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Ok(Self {
            engine,
            plugins: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: Some(event_rx),
            hook_registry: Arc::new(RwLock::new(HookRegistry {
                hooks: HashMap::new(),
            })),
        })
    }

    /// Load a plugin from a WASM file
    pub async fn load_plugin(&mut self, path: &Path) -> Result<String, FerrixError> {
        let wasm_bytes = std::fs::read(path)
            .map_err(|e| FerrixError::Plugin(format!("Failed to read plugin file: {}", e)))?;

        let module = Module::new(&self.engine, &wasm_bytes)
            .map_err(|e| FerrixError::Plugin(format!("Failed to compile WASM module: {}", e)))?;

        // Create WASI context
        let wasi_p1_ctx = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build_p1();

        // Get plugin manifest
        let manifest = self.get_plugin_manifest(&module).await?;

        // Verify API compatibility
        if manifest.api_version != API_VERSION {
            return Err(FerrixError::Plugin(format!(
                "Plugin API version {} is incompatible with runtime version {}",
                manifest.api_version, API_VERSION
            )));
        }

        let plugin_id = uuid::Uuid::new_v4().to_string();

        // Create plugin state
        let plugin_state = PluginState {
            wasi: wasi_p1_ctx,
            context: PluginContext {
                session_id: None,
                window_id: None,
                pane_id: None,
                user_data: HashMap::new(),
            },
            manifest: manifest.clone(),
            event_queue: Vec::new(),
        };

        let store = Arc::new(Mutex::new(Store::new(&self.engine, plugin_state)));

        // Create linker and add WASI
        let mut linker = Linker::new(&self.engine);
        // Add WASI to linker - updated for wasmtime 27.0
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state: &mut PluginState| &mut state.wasi)?;

        // Add Ferrix API functions
        self.add_ferrix_api(&mut linker)?;

        // Instantiate the module
        let instance = {
            let mut store_guard = store.lock().unwrap();
            linker.instantiate(&mut *store_guard, &module)
                .map_err(|e| FerrixError::Plugin(format!("Failed to instantiate plugin: {}", e)))?
        };

        // Get exported functions
        let mut exports = HashMap::new();
        {
            let mut store_guard = store.lock().unwrap();
            for export_name in &manifest.exports {
                if let Some(func) = instance.get_func(&mut *store_guard, export_name) {
                    exports.insert(export_name.clone(), func);
                }
            }
        }

        // Initialize the plugin
        {
            let mut store_guard = store.lock().unwrap();
            if let Some(init_func) = instance.get_func(&mut *store_guard, "plugin_init") {
                init_func.call(&mut *store_guard, &[], &mut [])
                    .map_err(|e| FerrixError::Plugin(format!("Plugin initialization failed: {}", e)))?;
            }
        }

        let loaded_plugin = LoadedPlugin {
            id: plugin_id.clone(),
            manifest: manifest.clone(),
            instance,
            store,
            exports,
        };

        // Register hooks
        self.register_plugin_hooks(&plugin_id, &manifest).await;

        // Store the plugin
        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.clone(), loaded_plugin);

        info!("Loaded plugin: {} v{}", manifest.name, manifest.version);

        // Notify other plugins
        self.broadcast_event(PluginEvent::PluginLoaded {
            plugin_name: manifest.name.clone(),
        }).await;

        Ok(plugin_id)
    }

    /// Unload a plugin
    pub async fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), FerrixError> {
        let mut plugins = self.plugins.write().await;

        if let Some(plugin) = plugins.remove(plugin_id) {
            // Call cleanup if available
            if let Some(cleanup_func) = plugin.exports.get("plugin_cleanup") {
                let mut store_guard = plugin.store.lock().unwrap();
                cleanup_func.call(&mut *store_guard, &[], &mut [])
                    .map_err(|e| FerrixError::Plugin(format!("Plugin cleanup failed: {}", e)))?;
            }

            // Unregister hooks
            self.unregister_plugin_hooks(plugin_id).await;

            info!("Unloaded plugin: {}", plugin.manifest.name);

            // Notify other plugins
            self.broadcast_event(PluginEvent::PluginUnloaded {
                plugin_name: plugin.manifest.name,
            }).await;

            Ok(())
        } else {
            Err(FerrixError::Plugin(format!("Plugin not found: {}", plugin_id)))
        }
    }

    /// Execute a command through plugins
    pub async fn execute_command(
        &self,
        command: PluginCommand,
        context: PluginContext,
    ) -> Result<PluginResponse, FerrixError> {
        let plugins = self.plugins.read().await;

        // Execute command on all plugins that export "handle_command"
        for (_id, plugin) in plugins.iter() {
            if let Some(handle_func) = plugin.exports.get("handle_command") {
                // Get a lock on the store and update context
                let mut store_guard = plugin.store.lock().unwrap();
                store_guard.data_mut().context = context.clone();

                // Serialize command to pass to WASM
                let command_json = serde_json::to_string(&command)
                    .map_err(|e| FerrixError::Plugin(format!("Failed to serialize command: {}", e)))?;

                // For now, call the function without parameters
                // In a real implementation, you'd pass the serialized command to WASM memory
                match handle_func.call(&mut *store_guard, &[], &mut []) {
                    Ok(_) => {
                        return Ok(PluginResponse::Success { data: None });
                    }
                    Err(e) => {
                        warn!("Plugin command execution failed: {}", e);
                        continue;
                    }
                }
            }
        }

        Err(FerrixError::Plugin("No plugin handled the command".to_string()))
    }

    /// Trigger a hook with context
    pub async fn trigger_hook(
        &self,
        hook: PluginHook,
        context: PluginContext,
    ) -> Result<Vec<PluginResponse>, FerrixError> {
        let hook_registry = self.hook_registry.read().await;
        let plugins = self.plugins.read().await;
        let mut responses = Vec::new();

        if let Some(plugin_ids) = hook_registry.hooks.get(&hook) {
            for plugin_id in plugin_ids {
                if let Some(plugin) = plugins.get(plugin_id) {
                    // Call hook handler if it exists
                    let hook_name = format!("hook_{:?}", hook).to_lowercase();
                    if let Some(hook_func) = plugin.exports.get(&hook_name) {
                        // Get a lock on the store and update context
                        let mut store_guard = plugin.store.lock().unwrap();
                        store_guard.data_mut().context = context.clone();

                        // Call the hook function
                        match hook_func.call(&mut *store_guard, &[], &mut []) {
                            Ok(_) => {
                                responses.push(PluginResponse::Success { data: None });
                            }
                            Err(e) => {
                                warn!("Plugin hook execution failed: {}", e);
                                responses.push(PluginResponse::Error {
                                    message: format!("Hook execution failed: {}", e)
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(responses)
    }

    /// Broadcast an event to all plugins
    pub async fn broadcast_event(&self, event: PluginEvent) {
        let plugins = self.plugins.read().await;

        for (_id, plugin) in plugins.iter() {
            if let Some(event_func) = plugin.exports.get("handle_event") {
                // Get a lock on the store and add event to queue
                let mut store_guard = plugin.store.lock().unwrap();
                store_guard.data_mut().event_queue.push(event.clone());

                // Call the event handler function
                match event_func.call(&mut *store_guard, &[], &mut []) {
                    Ok(_) => {
                        // Event handled successfully
                    }
                    Err(e) => {
                        warn!("Plugin event handling failed: {}", e);
                    }
                }
            }
        }
    }

    /// Get list of loaded plugins
    pub async fn list_plugins(&self) -> Vec<PluginManifest> {
        let plugins = self.plugins.read().await;
        plugins.values().map(|p| p.manifest.clone()).collect()
    }

    // Helper functions

    async fn get_plugin_manifest(
        &self,
        module: &Module,
    ) -> Result<PluginManifest, FerrixError> {
        // Create temporary store to call get_manifest
        let wasi_p1_ctx = WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build_p1();

        let mut store = Store::new(&self.engine, PluginState {
            wasi: wasi_p1_ctx,
            context: PluginContext {
                session_id: None,
                window_id: None,
                pane_id: None,
                user_data: HashMap::new(),
            },
            manifest: PluginManifest {
                name: String::new(),
                version: String::new(),
                author: String::new(),
                description: String::new(),
                homepage: None,
                license: None,
                api_version: String::new(),
                capabilities: Vec::new(),
                exports: Vec::new(),
            },
            event_queue: Vec::new(),
        });

        let mut linker = Linker::new(&self.engine);
        // Add WASI to linker - updated for wasmtime 27.0
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state: &mut PluginState| &mut state.wasi)?;

        let instance = linker.instantiate(&mut store, module)
            .map_err(|e| FerrixError::Plugin(format!("Failed to get manifest: {}", e)))?;

        // Call get_manifest function
        let get_manifest_func = instance
            .get_func(&mut store, "get_manifest")
            .ok_or_else(|| FerrixError::Plugin("Plugin missing get_manifest export".to_string()))?;

        get_manifest_func.call(&mut store, &[], &mut [])
            .map_err(|e| FerrixError::Plugin(format!("Failed to get manifest: {}", e)))?;

        // The plugin should have set the manifest in memory
        // This is simplified - actual implementation would need proper memory reading
        Ok(store.data().manifest.clone())
    }

    fn add_ferrix_api(&self, linker: &mut Linker<PluginState>) -> Result<()> {
        // Add Ferrix API functions that plugins can call

        // Log function
        linker.func_wrap("ferrix", "log", |mut caller: wasmtime::Caller<'_, PluginState>, level: i32, ptr: i32, len: i32| {
            // Read string from WASM memory
            // This is simplified - actual implementation would need proper memory management
            match level {
                0 => info!("Plugin log: (message at {}:{})", ptr, len),
                1 => warn!("Plugin warning: (message at {}:{})", ptr, len),
                2 => error!("Plugin error: (message at {}:{})", ptr, len),
                _ => {}
            }
        })?;

        // Send command function
        linker.func_wrap("ferrix", "send_command", |mut caller: wasmtime::Caller<'_, PluginState>, ptr: i32, len: i32| -> i32 {
            // Read command from WASM memory and execute
            // Return response code
            0 // Success
        })?;

        // Get context function
        linker.func_wrap("ferrix", "get_context", |mut caller: wasmtime::Caller<'_, PluginState>| -> i32 {
            // Write current context to WASM memory
            // Return pointer to context data
            0
        })?;

        Ok(())
    }

    async fn register_plugin_hooks(&self, plugin_id: &str, manifest: &PluginManifest) {
        let mut registry = self.hook_registry.write().await;

        // Register hooks based on exported functions
        for export in &manifest.exports {
            if export.starts_with("hook_") {
                // Parse hook type from function name
                let hook_name = export.strip_prefix("hook_").unwrap();

                // Map to PluginHook enum (simplified)
                let hook = match hook_name {
                    "pre_session_create" => PluginHook::PreSessionCreate,
                    "post_session_create" => PluginHook::PostSessionCreate,
                    "pre_window_create" => PluginHook::PreWindowCreate,
                    "post_window_create" => PluginHook::PostWindowCreate,
                    _ => continue,
                };

                registry.hooks
                    .entry(hook)
                    .or_insert_with(Vec::new)
                    .push(plugin_id.to_string());
            }
        }
    }

    async fn unregister_plugin_hooks(&self, plugin_id: &str) {
        let mut registry = self.hook_registry.write().await;

        for (_hook, plugins) in registry.hooks.iter_mut() {
            plugins.retain(|id| id != plugin_id);
        }
    }
}