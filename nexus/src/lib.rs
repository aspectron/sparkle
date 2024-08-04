cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {

        pub mod context;
        pub mod error;
        pub mod imports;
        #[allow(clippy::module_inception)]
        pub mod nexus;
        pub mod event;
        pub mod analyzer;
        pub mod result;
        pub mod processor;
        pub mod utils;

        pub mod prelude {
            pub use crate::nexus::Nexus;
            pub use crate::analyzer::Analyzer;
            pub use crate::context::ContextT;
        }
    }
}
