# Ferrix Feature Flags - Build What You Need

**Philosophy**: Ferrix embraces feature flags to let **you** control complexity. Want a minimal tmux-like experience? Use the defaults. Want bleeding-edge features? Enable them selectively.

---

## 🎯 Quick Start

```bash
# Minimal build (5-6MB) - Core multiplexing only
cargo build --release

# Recommended build - Most users want this
cargo build --release --features essential

# Power user build - Advanced features
cargo build --release --features power-user

# Everything enabled - All the bells and whistles
cargo build --release --features full
```

---

## 📦 Feature Tiers

### TIER 1: Core Features (Default)
**Status**: ✅ Stable, well-tested, minimal dependencies

```toml
default = ["clipboard", "scrollback"]
```

- ✅ **clipboard** - Copy/paste integration
- ✅ **scrollback** - Terminal scrollback buffer

---

### TIER 2: Advanced Features
**Status**: ✅ Stable, production-ready

- **recording** - Session recording & replay
- **remote** - TCP/TLS remote access  
- **performance** - Output optimization

---

### TIER 3: Experimental Features
**Status**: ⚠️ May have rough edges

- **versioning** - Git-like session versioning
- **collaboration** - Real-time collaborative editing
- **time-travel** - Time-travel debugging
- **plugin** - WASM plugin system
- **ai-assist** - AI command suggestions

---

### TIER 4: UI Enhancements
**Status**: ⚠️ Cosmetic, high cost

- **gpu** - GPU-accelerated rendering (23MB+ deps)
- **battery-status** - Battery indicator (**has security vulnerability**)

---

## 🎨 Feature Groups

- **essential** = clipboard + scrollback + recording (~6-7MB)
- **power-user** = essential + remote + versioning + performance (~8-9MB)
- **full** = everything enabled (~9.7MB)

---

See full documentation and use cases in this file.
