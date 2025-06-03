use clap::{Parser, Subcommand};
use std::env::home_dir;
mod kubeconfig;
use crate::kubeconfig::{KubeConfig, Preferences};
use log::{info, warn};

fn default_kubeconfig_path() -> std::path::PathBuf {
    let p = home_dir().unwrap();
    let mut p = p.into_os_string();
    p.push("/.kube/config");
    return p.into();
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // /// Name of the person to greet
    // #[arg(short, long)]
    // name: String,

    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // count: u8,
    /// The path to the main kubeconfig file.
    #[arg(short, long, default_value_os_t = default_kubeconfig_path())]
    config: std::path::PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Merge a given kubeconfig with main
    Merge {
        /// Path to the kubeconfig file to merge into the main.
        #[arg(short, long)]
        other: std::path::PathBuf,

        /// Force and override existing values with the given ones.
        #[arg(short, long, default_value_t = false)]
        force: bool,

        /// Merge preferences from the given other kubeconfig.
        #[arg(long, default_value_t = false)]
        include_preferences: bool,
    },

    /// List all clusters in the kubeconfig.
    List {
        /// Include the currently selected namespace.
        #[arg(short, long, default_value_t = false)]
        long: bool,
    },

    /// Delete the given cluster in the kubeconfig.
    Delete {
        /// The cluster name to delete from the kubeconfig.
        #[arg(short, long)]
        cluster: String,

        /// Skip interactive confirmation.
        #[arg(short, long, default_value_t = false)]
        yes: bool,
    },
}

#[derive(Debug)]
pub enum KubeCfgError {
    MergeError(String),
}

fn merge_kubeconfigs(
    main: KubeConfig,
    other: KubeConfig,
    force: bool,
    include_preferences: bool,
) -> Result<KubeConfig, KubeCfgError> {
    let mut main = main;

    // Merge preferences
    if include_preferences {
        match other.preferences {
            Some(other_preferences) => {
                match main.preferences {
                    Some(main_preferences) => {
                        // Some values already exist in main. Merge depending on force.
                        let mut merged_preferences = Preferences::default();

                        // Merge colors
                        match main_preferences.colors {
                            Some(colors) => {
                                if force {
                                    warn!("Merging preferences colors value to `{:?}` even though main had it set to `{}` because of --force flag.", other_preferences.colors, colors);
                                    // If force, take the value from other.
                                    merged_preferences.colors = other_preferences.colors;
                                } else {
                                    merged_preferences.colors = Some(colors);
                                }
                            }
                            None => {
                                // No colors in main, just apply other.
                                merged_preferences.colors = other_preferences.colors;
                            }
                        }

                        // Merge extensions
                        let mut merged_extensions = main_preferences.extensions;
                        for other_extension in other_preferences.extensions {
                            let existing_extensions_index = merged_extensions
                                .iter()
                                .position(|e| e.name == other_extension.name);
                            if existing_extensions_index.is_none() {
                                merged_extensions.push(other_extension);
                            } else if let Some(existing_extensions_index) =
                                existing_extensions_index
                            {
                                if force {
                                    warn!("Overriding preferences extension with name {} because of --force flag.", other_extension.name);
                                    merged_extensions[existing_extensions_index] = other_extension;
                                }
                            }
                        }
                        merged_preferences.extensions = merged_extensions;

                        // Set back to main.
                        main.preferences = Some(merged_preferences);
                    }
                    None => {
                        // No override would happen, so just take the new value.
                        main.preferences = Some(other_preferences);
                    }
                }
            }
            None => {}
        }
    }

    // Merge clusters.
    let mut merged_clusters = main.clusters;
    for other_cluster in other.clusters {
        let existing_clusters_index = merged_clusters
            .iter()
            .position(|e| e.name == other_cluster.name);
        if existing_clusters_index.is_none() {
            merged_clusters.push(other_cluster);
        } else if let Some(existing_clusters_index) = existing_clusters_index {
            if force {
                warn!("Overriding cluster with name {} because of --force flag.", other_cluster.name);
                merged_clusters[existing_clusters_index] = other_cluster;
            }
        }
    }
    // Set back to main.
    main.clusters = merged_clusters;

    // Merge users.
    let mut merged_users = main.users;
    for other_user in other.users {
        let existing_users_index = merged_users
            .iter()
            .position(|e| e.name == other_user.name);
        if existing_users_index.is_none() {
            merged_users.push(other_user);
        } else if let Some(existing_users_index) = existing_users_index {
            if force {
                warn!("Overriding user with name {} because of --force flag.", other_user.name);
                merged_users[existing_users_index] = other_user;
            }
        }
    }
    // Set back to main.
    main.users = merged_users;

    // Merge contexts.
    let mut merged_contexts = main.contexts;
    for other_context in other.contexts {
        let existing_contexts_index = merged_contexts
            .iter()
            .position(|e| e.name == other_context.name);
        if existing_contexts_index.is_none() {
            merged_contexts.push(other_context);
        } else if let Some(existing_contexts_index) = existing_contexts_index {
            if force {
                warn!("Overriding context with name {} because of --force flag.", other_context.name);
                merged_contexts[existing_contexts_index] = other_context;
            }
        }
    }
    // Set back to main.
    main.contexts = merged_contexts;

    // Merge extensions.
    let mut merged_extensions = main.extensions;
    for other_extension in other.extensions {
        let existing_extensions_index = merged_extensions
            .iter()
            .position(|e| e.name == other_extension.name);
        if existing_extensions_index.is_none() {
            merged_extensions.push(other_extension);
        } else if let Some(existing_extensions_index) = existing_extensions_index {
            if force {
                warn!("Overriding extension with name {} because of --force flag.", other_extension.name);
                merged_extensions[existing_extensions_index] = other_extension;
            }
        }
    }
    // Set back to main.
    main.extensions = merged_extensions;

    return Ok(main);
}

fn main() {
    let args = Args::parse();

    let kubeconfig = match KubeConfig::from_file(&args.config) {
        Ok(k) => k,
        Err(e) => panic!(
            "Main kubeconfig with path: {} - could not be verified due to error: {}",
            args.config.display(),
            e
        ),
    };

    // let serialized = serde_yaml::to_string(&kubeconfig).ok();
    // println!("{}", serialized.unwrap());

    match args.command {
        Commands::Merge {
            other,
            force,
            include_preferences,
        } => {
            let other_kubeconfig = match KubeConfig::from_file(&other) {
                Ok(k) => k,
                Err(e) => panic!(
                    "Other kubeconfig (to merge) with path: {} - could not be verified due to error: {}",
                    other.display(),
                    e
                ),
            };

            let merged_kubeconfig =
                merge_kubeconfigs(kubeconfig, other_kubeconfig, force, include_preferences);

            info!("Writing merged kubeconfig to original given kubeconfig location.");
            println!("{}", serde_yaml::to_string(&merged_kubeconfig.ok()).ok().unwrap())
        }
        _ => {}
    }

    // for _ in 0..args.count {
    //     println!("Hello {}!", args.name);
    // }
}
