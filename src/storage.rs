use serde::{Serialize, de::DeserializeOwned};

pub const LIBRARY_KEY: &str = "easy-ref.library";
pub const SYNC_KEY: &str = "easy-ref.sync";
pub const LANG_KEY: &str = "easy-ref.lang";
pub const THEME_KEY: &str = "easy-ref.theme";
pub const SIDEBAR_KEY: &str = "easy-ref.sidebar-width";
pub const COLLAPSED_KEY: &str = "easy-ref.collapsed";
pub const COLUMNS_KEY: &str = "easy-ref.columns";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn load<T: DeserializeOwned + Default>(key: &str) -> T {
    local_storage()
        .and_then(|s| s.get_item(key).ok().flatten())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save<T: Serialize>(key: &str, value: &T) {
    if let (Some(storage), Ok(raw)) = (local_storage(), serde_json::to_string(value)) {
        let _ = storage.set_item(key, &raw);
    }
}

pub fn now() -> f64 {
    js_sys::Date::now()
}

pub fn new_id() -> String {
    let rand = (js_sys::Math::random() * 1e15) as u64;
    format!("{:x}{:x}", now() as u64, rand)
}
