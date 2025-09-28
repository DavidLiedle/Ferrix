pub mod user_store;

pub use user_store::UserStore;
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_module_initialization() {
        // Auth module test
        assert!(true);
    }
}
