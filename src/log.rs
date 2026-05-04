#[cfg(not(feature = "github_actions_bindings"))]
pub(crate) mod console {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console, js_name = log)]
        pub fn log(s: &str);

        #[wasm_bindgen(js_namespace = console, js_name = warn)]
        pub fn warn(s: &str);

        #[wasm_bindgen(js_namespace = console, js_name = error)]
        pub fn error(s: &str);
    }
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        // Formats and outputs a GitHub actions log line at debug level
        #[cfg(feature = "github_actions_bindings")]
        $crate::actions::core::debug(std::format!($($arg)*).as_str());
        #[cfg(not(feature = "github_actions_bindings"))]
        $crate::log::console::log(&std::format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        // Formats and outputs a GitHub actions log line at info level
        #[cfg(feature = "github_actions_bindings")]
        $crate::actions::core::info(std::format!($($arg)*).as_str());
        #[cfg(not(feature = "github_actions_bindings"))]
        $crate::log::console::log(&std::format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! notice {
    ($($arg:tt)*) => {{
        // Formats and outputs a GitHub actions log line at notice level (this will be an annotation)
        #[cfg(feature = "github_actions_bindings")]
        $crate::actions::core::notice(std::format!($($arg)*).as_str());
        #[cfg(not(feature = "github_actions_bindings"))]
        $crate::log::console::log(&std::format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => {{
        // Formats and outputs a GitHub actions log line at warning level (this will be an annotation)
        #[cfg(feature = "github_actions_bindings")]
        $crate::actions::core::warning(std::format!($($arg)*).as_str());
        #[cfg(not(feature = "github_actions_bindings"))]
        $crate::log::console::warn(&std::format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        // Formats and outputs a GitHub actions log line at error level (this will be an annotation)
        #[cfg(feature = "github_actions_bindings")]
        $crate::actions::core::error(std::format!($($arg)*).as_str());
        #[cfg(not(feature = "github_actions_bindings"))]
        $crate::log::console::error(&std::format!($($arg)*));
    }};
}

#[cfg(test)]
mod tests {
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn logging_macros_callable() {
        debug!("debug {}", 1);
        info!("info {}", 2);
        notice!("notice {}", 3);
        warning!("warning {}", 4);
        error!("error {}", 5);
    }
}
