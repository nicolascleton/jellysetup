use std::collections::HashMap;
use regex::Regex;

/// Variables disponibles pour le remplacement dans les templates
pub struct TemplateVars {
    vars: HashMap<String, String>,
}

impl TemplateVars {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    /// Ajoute une variable
    pub fn set(&mut self, key: &str, value: &str) {
        self.vars.insert(key.to_string(), value.to_string());
    }

    /// Remplace toutes les variables {{VAR}} dans une chaîne
    pub fn replace(&self, template: &str) -> String {
        let re = Regex::new(r"\{\{([A-Z_0-9]+)\}\}").unwrap();

        re.replace_all(template, |caps: &regex::Captures| {
            let var_name = &caps[1];
            match self.vars.get(var_name) {
                Some(value) => value.clone(),
                None => {
                    println!("[Template] Warning: Variable {{{{{}}}}} not found, replacing with empty string", var_name);
                    String::new()
                }
            }
        }).to_string()
    }

    /// Remplace les variables dans un objet JSON
    pub fn replace_in_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => {
                serde_json::Value::String(self.replace(s))
            }
            serde_json::Value::Array(arr) => {
                serde_json::Value::Array(
                    arr.iter().map(|v| self.replace_in_json(v)).collect()
                )
            }
            serde_json::Value::Object(obj) => {
                serde_json::Value::Object(
                    obj.iter()
                        .map(|(k, v)| (k.clone(), self.replace_in_json(v)))
                        .collect()
                )
            }
            other => other.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_replacement() {
        let mut vars = TemplateVars::new();
        vars.set("PI_IP", "192.168.1.100");
        vars.set("API_KEY", "test-key-123");

        let template = "http://{{PI_IP}}:8096/api?key={{API_KEY}}";
        assert_eq!(
            vars.replace(template),
            "http://192.168.1.100:8096/api?key=test-key-123"
        );
    }

    #[test]
    fn test_unknown_var() {
        let vars = TemplateVars::new();
        let template = "{{UNKNOWN}}";
        assert_eq!(vars.replace(template), "");
    }

    #[test]
    fn test_json_replacement() {
        let mut vars = TemplateVars::new();
        vars.set("PI_IP", "192.168.1.100");

        let json = serde_json::json!({
            "hostname": "{{PI_IP}}",
            "port": 8096,
            "nested": {
                "url": "http://{{PI_IP}}"
            }
        });

        let result = vars.replace_in_json(&json);
        assert_eq!(result["hostname"], "192.168.1.100");
        assert_eq!(result["nested"]["url"], "http://192.168.1.100");
    }
}
