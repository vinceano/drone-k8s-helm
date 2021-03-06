extern crate serde_json;

#[cfg(test)]
pub mod tests;

use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::collections::BTreeMap;
use std::process::Command;

use handlebars::Handlebars;
use serde_json::Value;
use regex::Regex;

use utils;
use plugin::Plugin;

const TEMPLATE: &'static str = "\
apiVersion: v1
clusters:
- cluster:
    insecure-skip-tls-verify: {{ skip_tls }}
    server: {{ master }}
  name: helm
contexts:
- context:
    cluster: helm
    namespace: {{ namespace }}
    user: helm
  name: helm
current-context: helm
kind: Config
preferences: {}
users:
- name: helm
  user:
    token: {{ token }}\
";

#[derive(Debug)]
pub struct Config {
    pub chart: Value,
    pub master: Value,
    pub namespace: Value,
    pub release: Value,
    pub skip_tls: Value,
    pub token: Value,
    pub clean_before_release: Value,
    pub values: Value,
}

impl Plugin for Config {
    fn build_clean_command(&self) -> Command {
        let kubectl_bin = utils::which("kubectl").expect("Kubernetes CLI to be installed");
        let mut command = Command::new(kubectl_bin.to_str().unwrap());

        command.arg("delete").arg("jobs").arg("-l").arg(format!(
            "release={}",
            self.release
                .as_str()
                .unwrap()
        ));
        command
    }

    fn build_upgrade_command(&self) -> Command {
        let helm_bin = utils::which("helm").expect("Helm to be installed");
        let mut command = Command::new(helm_bin.to_str().unwrap());

        command.arg("upgrade").arg("-i").arg(
            self.release
                .as_str()
                .unwrap(),
        );

        for (key, value) in self.values.as_object().unwrap() {
            command.arg("--set").arg(
                format!(
                    "{}={}",
                    key,
                    value.as_str().unwrap()
                ).as_str(),
            );
        }

        command.arg(self.chart.as_str().unwrap());
        command
    }
}

impl Config {
    pub fn new() -> Config {
        let mut config = Config::default();

        config.load();
        config.parse_values();
        config.write_file();

        config
    }

    pub fn default() -> Config {
        Config {
            chart: Value::Null,
            master: Value::Null,
            namespace: Value::String("default".to_string()),
            release: Value::Null,
            skip_tls: Value::Bool(false),
            token: Value::Null,
            clean_before_release: Value::Bool(false),
            values: Value::Null,
        }
    }

    pub fn load(&mut self) -> () {
        self.chart = env::var("PLUGIN_CHART")
            .or(env::var("HELM_CHART"))
            .and_then(|chart| Ok(Value::String(chart)))
            .expect("HELM_CHART env must be set");
        self.master = env::var("PLUGIN_MASTER")
            .or(env::var("HELM_MASTER"))
            .and_then(|master| Ok(Value::String(master)))
            .expect("HELM_MASTER env must be set");
        self.namespace = env::var("PLUGIN_NAMESPACE")
            .or(env::var("HELM_NAMESPACE"))
            .and_then(|namespace| Ok(Value::String(namespace)))
            .unwrap_or_default();
        self.release = env::var("PLUGIN_RELEASE")
            .or(env::var("HELM_RELEASE"))
            .and_then(|release| Ok(Value::String(release)))
            .expect("HELM_RELEASE env must be set");
        self.skip_tls = env::var("PLUGIN_SKIP_TLS")
            .or(env::var("HELM_SKIP_TLS"))
            .and_then(|skip_tls| {
                Ok(Value::Bool(
                    skip_tls.parse().expect("HELM_SKIP_TLS must be bool"),
                ))
            })
            .unwrap_or_default();
        self.token = env::var("PLUGIN_TOKEN")
            .or(env::var("HELM_TOKEN"))
            .and_then(|token| Ok(Value::String(token)))
            .expect("HELM_TOKEN env must be set");
        self.clean_before_release = env::var("PLUGIN_CLEAN_BEFORE_RELEASE")
            .or(env::var("HELM_CLEAN_BEFORE_RELEASE"))
            .and_then(|clean_before_release| {
                Ok(Value::Bool(clean_before_release.parse().expect(
                    "HELM_CLEAN_BEFORE_RELEASE must be bool",
                )))
            })
            .unwrap_or_default();
    }

    pub fn parse_values(&mut self) -> () {
        let regex = Regex::new(r"^\{\{(\w+)\}\}$").unwrap();
        let data: String = env::var("PLUGIN_VALUES")
            .or(env::var("HELM_VALUES"))
            .unwrap_or("{}".to_string());

        self.values = serde_json::from_str::<Value>(&data).expect("Failed to parse values");

        for (_, value) in self.values.as_object_mut().expect(
            "Values must be an object",
        )
        {
            let value_string = value.as_str().unwrap().to_string();

            if regex.is_match(&value_string) {
                let captured = regex
                    .captures(&value_string)
                    .unwrap()
                    .get(1)
                    .unwrap()
                    .as_str();
                let var = env::var(captured).expect(format!("{} is not set", captured).as_str());
                *value = Value::String(var.to_string());
            }
        }
    }

    fn create_file(&self) -> File {
        let mut config_path = env::home_dir().expect("Failed to find home directory");

        config_path.push(".kube");

        fs::create_dir_all(config_path.as_path()).expect("Failed to create config directory");

        config_path.push("config");

        File::create(config_path).expect("Failed to create config file")
    }

    fn write_file(&self) -> () {
        let rendered_config = self.render_file();

        self.create_file()
            .write(&rendered_config.into_bytes())
            .expect("Failed to write config");
    }

    fn render_file(&self) -> String {
        let mut handlebars = Handlebars::new();
        let mut assigns = BTreeMap::new();

        handlebars
            .register_template_string("config", TEMPLATE)
            .expect("Failed to register template");

        assigns.insert("master", &self.master);
        assigns.insert("namespace", &self.namespace);
        assigns.insert("skip_tls", &self.skip_tls);
        assigns.insert("token", &self.token);

        handlebars.render("config", &assigns).expect(
            "Failed to render kube config",
        )
    }
}
