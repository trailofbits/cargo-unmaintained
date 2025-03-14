use serde_json::{Map, Value};

#[allow(dead_code)]
pub(crate) trait MapExt {
    fn get_array(&self, key: &str) -> Option<&Vec<Value>>;
    fn get_bool(&self, key: &str) -> Option<bool>;
    fn get_object(&self, key: &str) -> Option<&Map<String, Value>>;
    fn get_str(&self, key: &str) -> Option<&str>;
}

impl MapExt for Map<String, Value> {
    fn get_array(&self, key: &str) -> Option<&Vec<Value>> {
        self.get(key).and_then(Value::as_array)
    }
    fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }
    fn get_object(&self, key: &str) -> Option<&Map<String, Value>> {
        self.get(key).and_then(Value::as_object)
    }
    fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
}
