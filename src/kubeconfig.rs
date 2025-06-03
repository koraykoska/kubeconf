use serde::{Deserialize, Serialize};
use serde_yaml::{self, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use base64::{self, Engine};

/// Spec according to https://kubernetes.io/docs/reference/config-api/kubeconfig.v1/
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct KubeConfig {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Preferences>,

    #[serde(default)]
    pub clusters: Vec<NamedCluster>,

    #[serde(default)]
    pub users: Vec<NamedUser>,

    #[serde(default)]
    pub contexts: Vec<NamedContext>,

    #[serde(rename = "current-context", skip_serializing_if = "Option::is_none")]
    pub current_context: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<NamedExtension>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<bool>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<NamedExtension>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedCluster {
    pub name: String,
    pub cluster: Cluster,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Cluster {
    pub server: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub insecure_skip_tls_verify: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_authority: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_authority_data: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_compression: Option<bool>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<NamedExtension>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedUser {
    pub name: String,
    pub user: User,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case",deny_unknown_fields)]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_certificate: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_certificate_data: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key_data: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[serde(rename = "tokenFile", skip_serializing_if = "Option::is_none")]
    pub token_file: Option<String>,

    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub impersonate: Option<String>,

    #[serde(rename = "as-uid", skip_serializing_if = "Option::is_none")]
    pub impersonate_uid: Option<String>,

    #[serde(rename = "as-groups", default, skip_serializing_if = "Vec::is_empty")]
    pub impersonate_groups: Vec<String>,

    #[serde(rename = "as-user-extra", default, skip_serializing_if = "HashMap::is_empty")]
    pub impersonate_user_extra: HashMap<String, Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_provider: Option<AuthProvider>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec: Option<ExecConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<NamedExtension>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthProvider {
    pub name: String,

    #[serde(default)]
    pub config: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecConfig {
    pub command: String,

    #[serde(default)]
    pub args: Option<Vec<String>>,

    #[serde(default)]
    pub env: Option<Vec<ExecEnvVar>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provide_cluster_info: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactive_mode: Option<InteractiveMode>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecEnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InteractiveMode {
    Never,
    IfAvailable,
    Always,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedContext {
    pub name: String,
    pub context: Context,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub cluster: String,
    pub user: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<NamedExtension>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedExtension {
    pub name: String,

    #[serde(default)]
    pub extension: Value,
}

// Validation and parsing functions
impl KubeConfig {
    /// Load and parse a kubeconfig from a file path
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, KubeConfigError> {
        let contents = fs::read_to_string(path)
            .map_err(|e| KubeConfigError::IoError(e))?;
        Self::from_yaml(&contents)
    }

    /// Parse a kubeconfig from a YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, KubeConfigError> {
        let config: KubeConfig = serde_yaml::from_str(yaml)
            .map_err(|e| KubeConfigError::ParseError(e))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate the kubeconfig
    pub fn validate(&self) -> Result<(), KubeConfigError> {
        // Check API version
        if self.api_version != "v1" {
            return Err(KubeConfigError::ValidationError(
                format!("Unsupported apiVersion: {}", self.api_version)
            ));
        }

        // Check kind
        if self.kind != "Config" {
            return Err(KubeConfigError::ValidationError(
                format!("Invalid kind: {}, expected 'Config'", self.kind)
            ));
        }

        // Validate current context exists if specified
        if let Some(ref current) = self.current_context {
            if !self.contexts.iter().any(|c| c.name == *current) {
                return Err(KubeConfigError::ValidationError(
                    format!("Current context '{}' not found in contexts", current)
                ));
            }
        }

        // Validate all contexts reference existing clusters and users
        for context in &self.contexts {
            if !self.clusters.iter().any(|c| c.name == context.context.cluster) {
                return Err(KubeConfigError::ValidationError(
                    format!("Context '{}' references non-existent cluster '{}'",
                            context.name, context.context.cluster)
                ));
            }

            if !self.users.iter().any(|u| u.name == context.context.user) {
                return Err(KubeConfigError::ValidationError(
                    format!("Context '{}' references non-existent user '{}'",
                            context.name, context.context.user)
                ));
            }
        }

        // Validate cluster configurations
        for cluster in &self.clusters {
            // Validate server URL
            if !cluster.cluster.server.starts_with("http://") &&
               !cluster.cluster.server.starts_with("https://") {
                return Err(KubeConfigError::ValidationError(
                    format!("Cluster '{}' has invalid server URL: {}",
                            cluster.name, cluster.cluster.server)
                ));
            }

            // Validate certificate data is base64 if provided
            if let Some(ref cert_data) = cluster.cluster.certificate_authority_data {
                base64::engine::general_purpose::STANDARD.decode(cert_data)
                    .map_err(|_| KubeConfigError::ValidationError(
                        format!("Cluster '{}' has invalid certificate-authority-data", cluster.name)
                    ))?;
            }
        }

        // Validate user configurations
        for user in &self.users {
            // Validate certificate data is base64 if provided
            if let Some(ref cert_data) = user.user.client_certificate_data {
                base64::engine::general_purpose::STANDARD.decode(cert_data)
                    .map_err(|_| KubeConfigError::ValidationError(
                        format!("User '{}' has invalid client-certificate-data", user.name)
                    ))?;
            }

            if let Some(ref key_data) = user.user.client_key_data {
                base64::engine::general_purpose::STANDARD.decode(key_data)
                    .map_err(|_| KubeConfigError::ValidationError(
                        format!("User '{}' has invalid client-key-data", user.name)
                    ))?;
            }
        }

        Ok(())
    }

    /// Get the current context
    pub fn get_current_context(&self) -> Option<&NamedContext> {
        self.current_context.as_ref()
            .and_then(|name| self.contexts.iter().find(|c| c.name == *name))
    }

    /// Get a context by name
    pub fn get_context(&self, name: &str) -> Option<&NamedContext> {
        self.contexts.iter().find(|c| c.name == name)
    }

    /// Get a cluster by name
    pub fn get_cluster(&self, name: &str) -> Option<&NamedCluster> {
        self.clusters.iter().find(|c| c.name == name)
    }

    /// Get a user by name
    pub fn get_user(&self, name: &str) -> Option<&NamedUser> {
        self.users.iter().find(|u| u.name == name)
    }
}

#[derive(Debug)]
pub enum KubeConfigError {
    IoError(std::io::Error),
    ParseError(serde_yaml::Error),
    ValidationError(String),
}

impl std::fmt::Display for KubeConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KubeConfigError::IoError(e) => write!(f, "IO error: {}", e),
            KubeConfigError::ParseError(e) => write!(f, "Parse error: {}", e),
            KubeConfigError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
        }
    }
}

impl std::error::Error for KubeConfigError {}
