use clap::{Parser, Subcommand};
use std::{
    env::home_dir,
    io::{Read, Write, stdin, stdout},
    path::PathBuf,
    process::exit,
    vec,
};
mod kubeconfig;
use crate::kubeconfig::{KubeConfig, NamedCluster, NamedContext, NamedUser, Preferences};
use colored::Colorize;
use log::{info, warn};
use regex::Regex;
use std::fs;
use tabled::{
    Table, Tabled,
    settings::{
        Color, Padding, Style,
        object::{Columns, Rows},
    },
};

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

        /// Only print the resulting merged kubeconfig file and do not write it to disk.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

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

    /// Rename a context, cluster or user gracefully.
    Rename {
        /// Rename a context from one value to the other. Syntax is previous value and new value separated by double colon.
        /// e.g.: previous-context-name::new-context-name
        #[arg(long)]
        context: Option<String>,

        /// Rename a cluster from one value to the other. Syntax is previous value and new value separated by double colon.
        /// e.g.: previous-cluster-name::new-cluster-name
        #[arg(long)]
        cluster: Option<String>,

        /// Rename a user from one value to the other. Syntax is previous value and new value separated by double colon.
        /// e.g.: previous-user-name::new-user-name
        #[arg(long)]
        user: Option<String>,

        /// Rename a context, cluster and user from one value to the other. Syntax is previous value and new value separated by double colon.
        /// e.g.: previous-user-name::new-user-name
        /// NOTE: `--context`, `--cluster` and `--user` are ignored if this is provided.
        #[arg(long)]
        all: Option<String>,

        /// Only print the resulting edited kubeconfig file and do not write it to disk.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Rename to new value even if there is an existing cluster/context/user with the given value.
        #[arg(short, long, default_value_t = false)]
        force: bool,
    },

    /// Delete the given cluster in the kubeconfig.
    Delete {
        /// The context name to delete from the kubeconfig.
        #[arg(short, long)]
        context: String,

        /// Only print the resulting edited kubeconfig file and do not write it to disk.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Skip interactive confirmation.
        #[arg(short, long, default_value_t = false)]
        yes: bool,
    },
}

#[derive(Debug)]
pub enum KubeConfError {
    MergeError(String),
}

#[derive(Tabled)]
struct PrettyPrintedContextNamespace {
    CONTEXT: String,
    NAMESPACE: String,
}

fn merge_kubeconfigs(
    main: KubeConfig,
    other: KubeConfig,
    force: bool,
    include_preferences: bool,
) -> Result<KubeConfig, KubeConfError> {
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
                                    warn!(
                                        "Merging preferences colors value to `{:?}` even though main had it set to `{}` because of --force flag.",
                                        other_preferences.colors, colors
                                    );
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
                                    warn!(
                                        "Overriding preferences extension with name {} because of --force flag.",
                                        other_extension.name
                                    );
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
                warn!(
                    "Overriding cluster with name {} because of --force flag.",
                    other_cluster.name
                );
                merged_clusters[existing_clusters_index] = other_cluster;
            }
        }
    }
    // Set back to main.
    main.clusters = merged_clusters;

    // Merge users.
    let mut merged_users = main.users;
    for other_user in other.users {
        let existing_users_index = merged_users.iter().position(|e| e.name == other_user.name);
        if existing_users_index.is_none() {
            merged_users.push(other_user);
        } else if let Some(existing_users_index) = existing_users_index {
            if force {
                warn!(
                    "Overriding user with name {} because of --force flag.",
                    other_user.name
                );
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
                warn!(
                    "Overriding context with name {} because of --force flag.",
                    other_context.name
                );
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
                warn!(
                    "Overriding extension with name {} because of --force flag.",
                    other_extension.name
                );
                merged_extensions[existing_extensions_index] = other_extension;
            }
        }
    }
    // Set back to main.
    main.extensions = merged_extensions;

    return Ok(main);
}

fn rename_kubeconfig_values(
    kubeconfig: KubeConfig,
    context: Option<String>,
    cluster: Option<String>,
    user: Option<String>,
    all: Option<String>,
    force: bool,
) -> KubeConfig {
    let mut kubeconfig = kubeconfig;

    let mut context = context;
    let mut cluster = cluster;
    let mut user = user;
    if let Some(all) = all {
        context = Some(all.clone());
        cluster = Some(all.clone());
        user = Some(all.clone());
    }

    if let Some(context) = context {
        let splitted_context: Vec<&str> = context.split("::").collect();
        if splitted_context.len() != 2 {
            panic!("`--context` needs to be in the syntax previous-value::new-value.")
        }

        let previous_value = splitted_context[0];
        let new_value = splitted_context[1];

        // Regex to check new value. We don't care about previous as we are replacing it anyways.
        // See https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#:~:text=DNS%20Subdomain%20Names,end%20with%20an%20alphanumeric%20character
        let regex = Regex::new(r"^([a-z0-9]{1})([a-z0-9\-\.]{0,251})([a-z0-9]{1})$").unwrap();
        if regex.captures(new_value).is_none() {
            panic!(
                "`--context` new value is not a valid name for the context. should be lowercase alphanumeric including hyphens and dots, start and end with alphanumeric only and be max. 253 characters long."
            );
        }

        // Find context with the new value, only start renaming if force in those cases.
        // kubeconfig file will be invalid if there are duplicates, so make sure force flag is set before doing this.
        if kubeconfig
            .contexts
            .iter()
            .find(|c| c.name == new_value)
            .is_some()
        {
            if force {
                warn!(
                    "Existing context with given new context name `{}` found in kubeconfig. Still renaming because of `--force` flag. WARN: THIS WILL RESULT IN AN INVALID KUBECONFIG FILE!",
                    new_value
                );
            } else {
                panic!(
                    "Existing context with given new context name `{}` found in kubeconfig. Refusing to rename. Add `--force` to force the rename, resulting in an invalid kubeconfig file.",
                    new_value
                );
            }
        }

        let mut number_of_renames = 0;
        let mut did_rename_current = false;
        let mut new_contexts: Vec<NamedContext> = vec![];
        for context in kubeconfig.contexts {
            if context.name == previous_value {
                let mut new_context = context;
                new_context.name = new_value.to_string();
                new_contexts.push(new_context);
                number_of_renames += 1;
            } else {
                new_contexts.push(context);
            }
        }
        kubeconfig.contexts = new_contexts;
        if kubeconfig.current_context == Some(previous_value.to_string()) {
            kubeconfig.current_context = Some(new_value.to_string());
            did_rename_current = true;
        }

        info!(
            "Renamed {} occurrences of `{}` context to `{}`",
            number_of_renames, previous_value, new_value
        );
        if did_rename_current {
            info!(
                "Renamed current_context from `{}` to `{}`",
                previous_value, new_value
            );
        }
    }

    if let Some(cluster) = cluster {
        let splitted_cluster: Vec<&str> = cluster.split("::").collect();
        if splitted_cluster.len() != 2 {
            panic!("`--cluster` needs to be in the syntax previous-value::new-value.")
        }

        let previous_value = splitted_cluster[0];
        let new_value = splitted_cluster[1];

        // Regex to check new value. We don't care about previous as we are replacing it anyways.
        // See https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#:~:text=DNS%20Subdomain%20Names,end%20with%20an%20alphanumeric%20character
        let regex = Regex::new(r"^([a-z0-9]{1})([a-z0-9\-\.]{0,251})([a-z0-9]{1})$").unwrap();
        if regex.captures(new_value).is_none() {
            panic!(
                "`--cluster` new value is not a valid name for the cluster. should be lowercase alphanumeric including hyphens and dots, start and end with alphanumeric only and be max. 253 characters long."
            );
        }

        // Find cluster with the new value, only start renaming if force in those cases.
        // kubeconfig file will be invalid if there are duplicates, so make sure force flag is set before doing this.
        if kubeconfig
            .clusters
            .iter()
            .find(|c| c.name == new_value)
            .is_some()
        {
            if force {
                warn!(
                    "Existing cluster with given new cluster name `{}` found in kubeconfig. Still renaming because of `--force` flag. WARN: THIS WILL RESULT IN AN INVALID KUBECONFIG FILE!",
                    new_value
                );
            } else {
                panic!(
                    "Existing cluster with given new cluster name `{}` found in kubeconfig. Refusing to rename. Add `--force` to force the rename, resulting in an invalid kubeconfig file.",
                    new_value
                );
            }
        }

        let mut number_of_renames = 0;
        let mut new_clusters: Vec<NamedCluster> = vec![];
        for cluster in kubeconfig.clusters {
            if cluster.name == previous_value {
                let mut new_cluster = cluster;
                new_cluster.name = new_value.to_string();
                new_clusters.push(new_cluster);
                number_of_renames += 1;
            } else {
                new_clusters.push(cluster);
            }
        }
        kubeconfig.clusters = new_clusters;
        let mut number_of_context_cluster_renames = 0;
        let mut new_contexts: Vec<NamedContext> = vec![];
        for context in kubeconfig.contexts {
            if context.context.cluster == previous_value {
                let mut new_context = context;
                new_context.context.cluster = new_value.to_string();
                new_contexts.push(new_context);
                number_of_context_cluster_renames += 1;
            } else {
                new_contexts.push(context);
            }
        }
        kubeconfig.contexts = new_contexts;

        info!(
            "Renamed {} occurrences of `{}` clusters to `{}`",
            number_of_renames, previous_value, new_value
        );
        info!(
            "Renamed {} occurrences of `{}` clusters in contexts to `{}`",
            number_of_context_cluster_renames, previous_value, new_value
        );
    }

    if let Some(user) = user {
        let splitted_user: Vec<&str> = user.split("::").collect();
        if splitted_user.len() != 2 {
            panic!("`--user` needs to be in the syntax previous-value::new-value.")
        }

        let previous_value = splitted_user[0];
        let new_value = splitted_user[1];

        // Regex to check new value. We don't care about previous as we are replacing it anyways.
        // See https://kubernetes.io/docs/concepts/overview/working-with-objects/names/#:~:text=DNS%20Subdomain%20Names,end%20with%20an%20alphanumeric%20character
        let regex = Regex::new(r"^([a-z0-9]{1})([a-z0-9\-\.]{0,251})([a-z0-9]{1})$").unwrap();
        if regex.captures(new_value).is_none() {
            panic!(
                "`--cluster` new value is not a valid name for the cluster. should be lowercase alphanumeric including hyphens and dots, start and end with alphanumeric only and be max. 253 characters long."
            );
        }

        // Find cluster with the new value, only start renaming if force in those cases.
        // kubeconfig file will be invalid if there are duplicates, so make sure force flag is set before doing this.
        if kubeconfig
            .users
            .iter()
            .find(|c| c.name == new_value)
            .is_some()
        {
            if force {
                warn!(
                    "Existing user with given new user name `{}` found in kubeconfig. Still renaming because of `--force` flag. WARN: THIS WILL RESULT IN AN INVALID KUBECONFIG FILE!",
                    new_value
                );
            } else {
                panic!(
                    "Existing user with given new user name `{}` found in kubeconfig. Refusing to rename. Add `--force` to force the rename, resulting in an invalid kubeconfig file.",
                    new_value
                );
            }
        }

        let mut number_of_renames = 0;
        let mut new_users: Vec<NamedUser> = vec![];
        for user in kubeconfig.users {
            if user.name == previous_value {
                let mut new_user = user;
                new_user.name = new_value.to_string();
                new_users.push(new_user);
                number_of_renames += 1;
            } else {
                new_users.push(user);
            }
        }
        kubeconfig.users = new_users;
        let mut number_of_context_cluster_renames = 0;
        let mut new_contexts: Vec<NamedContext> = vec![];
        for context in kubeconfig.contexts {
            if context.context.user == previous_value {
                let mut new_context = context;
                new_context.context.user = new_value.to_string();
                new_contexts.push(new_context);
                number_of_context_cluster_renames += 1;
            } else {
                new_contexts.push(context);
            }
        }
        kubeconfig.contexts = new_contexts;

        info!(
            "Renamed {} occurrences of `{}` users to `{}`",
            number_of_renames, previous_value, new_value
        );
        info!(
            "Renamed {} occurrences of `{}` users in contexts to `{}`",
            number_of_context_cluster_renames, previous_value, new_value
        );
    }

    return kubeconfig;
}

fn delete_context(kubeconfig: KubeConfig, context: String, yes: bool) -> KubeConfig {
    let old_number_of_contexts = kubeconfig.contexts.len();
    let old_number_of_clusters = kubeconfig.clusters.len();
    let old_number_of_users = kubeconfig.users.len();

    let mut kubeconfig = kubeconfig;

    let mut new_contexts: Vec<NamedContext> = vec![];
    let mut cluster_names_to_delete: Vec<String> = vec![];
    let mut user_names_to_delete: Vec<String> = vec![];
    for context_to_check in kubeconfig.contexts {
        if context_to_check.name == context {
            cluster_names_to_delete.push(context_to_check.context.cluster);
            user_names_to_delete.push(context_to_check.context.user);
        } else {
            new_contexts.push(context_to_check);
        }
    }
    kubeconfig.contexts = new_contexts;
    if kubeconfig.current_context == Some(context) {
        kubeconfig.current_context = None;
    }

    let mut new_clusters: Vec<NamedCluster> = vec![];
    for cluster_to_check in kubeconfig.clusters {
        if cluster_names_to_delete
            .iter()
            .find(|c| **c == cluster_to_check.name)
            .is_none()
        {
            new_clusters.push(cluster_to_check);
        }
    }
    kubeconfig.clusters = new_clusters;

    let mut new_users: Vec<NamedUser> = vec![];
    for user_to_check in kubeconfig.users {
        if user_names_to_delete
            .iter()
            .find(|c| **c == user_to_check.name)
            .is_none()
        {
            new_users.push(user_to_check);
        }
    }
    kubeconfig.users = new_users;

    if !yes {
        let mut s = String::new();
        println!(
            "This action is going to delete {} contexts, {} clusters and {} users.",
            old_number_of_contexts - kubeconfig.contexts.len(),
            old_number_of_clusters - kubeconfig.clusters.len(),
            old_number_of_users - kubeconfig.users.len(),
        );
        print!("Are you sure you want to continue? (y/n) ");
        let _ = stdout().flush();
        stdin()
            .read_line(&mut s)
            .expect("User input broken. Please try again.");

        if s.trim().to_lowercase() != "y" {
            // Terminate program.
            println!("User cancelled deleting the context.");
            exit(1);
        }
    }

    return kubeconfig;
}

fn write_kubeconfig(path: PathBuf, kubeconfig: KubeConfig, dry_run: bool) {
    match serde_yaml::to_string(&kubeconfig) {
        Ok(merged_kubeconfig_yaml) => {
            if dry_run {
                println!("{}", merged_kubeconfig_yaml);
            } else {
                match fs::write(&path, merged_kubeconfig_yaml) {
                    Ok(()) => {
                        // Done.
                    }
                    Err(error) => {
                        panic!("Writing kubeconfig yaml failed with error: {}", error);
                    }
                }
            }
        }
        Err(error) => {
            panic!("Converting kubeconfig to yaml failed with error: {}", error);
        }
    }
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
            dry_run,
        } => {
            let mut other_kubeconfig: Option<KubeConfig> = None;
            if let Some(path) = other.clone().into_os_string().to_str() {
                if path == "-" {
                    // Read from stdin.
                    let mut buffer = Vec::new();
                    let stdin = std::io::stdin();
                    let mut handle = stdin.lock();
                    match handle.read_to_end(&mut buffer) {
                        Ok(_size) => {
                            let s = match str::from_utf8(&buffer) {
                                Ok(v) => v,
                                Err(e) => panic!("invalid utf8 sequence in stdin: {}", e),
                            };

                            other_kubeconfig = match KubeConfig::from_yaml(s) {
                                Ok(k) => Some(k),
                                Err(e) => panic!(
                                    "Other kubeconfig (to merge) from stdin - could not be verified due to error: {}",
                                    e
                                ),
                            }
                        }
                        Err(e) => {
                            panic!("error while reading stdin: {}", e);
                        }
                    }
                }
            }

            if other_kubeconfig.is_none() {
                other_kubeconfig = match KubeConfig::from_file(&other) {
                    Ok(k) => Some(k),
                    Err(e) => panic!(
                        "Other kubeconfig (to merge) with path: {} - could not be verified due to error: {}",
                        other.display(),
                        e
                    ),
                };
            }

            let other_kubeconfig = other_kubeconfig.unwrap();

            match merge_kubeconfigs(kubeconfig, other_kubeconfig, force, include_preferences) {
                Ok(merged_kubeconfig) => {
                    info!("Writing merged kubeconfig to original given kubeconfig location.");

                    write_kubeconfig(args.config, merged_kubeconfig, dry_run);
                }
                Err(error) => {
                    panic!("Merging failed with error: {:?}", error);
                }
            }
        }
        Commands::List { long } => {
            let mut context_namespaces: Vec<PrettyPrintedContextNamespace> = vec![];

            let current_context = kubeconfig.current_context.unwrap_or("".to_string());
            let mut current_context_index = 0;
            let mut iterator = 0;
            for context in kubeconfig.contexts {
                let mut context_name = context.name;
                let mut context_namespace_name =
                    context.context.namespace.unwrap_or("default".to_string());
                if context_name == current_context {
                    if !long {
                        context_name = context_name.yellow().on_black().to_string();
                        context_namespace_name =
                            context_namespace_name.yellow().on_black().to_string();
                    }

                    current_context_index = iterator;
                }

                if long {
                    context_namespaces.push(PrettyPrintedContextNamespace {
                        CONTEXT: context_name.to_string(),
                        NAMESPACE: context_namespace_name.to_string(),
                    });
                } else {
                    println!("{}", context_name);
                }

                iterator += 1;
            }

            if long {
                let mut table = Table::new(context_namespaces);
                table.with(Style::blank());
                // Plus one because of the header.
                table.modify(
                    Rows::one(current_context_index + 1),
                    Color::BG_BLACK | Color::FG_YELLOW,
                );
                // table.with(Padding::zero());
                table.modify(Columns::first(), Padding::zero());

                // Print the table.
                println!("{}", table.to_string());
            }
        }
        Commands::Rename {
            context,
            cluster,
            user,
            all,
            dry_run,
            force,
        } => {
            let new_kubeconfig =
                rename_kubeconfig_values(kubeconfig, context, cluster, user, all, force);

            write_kubeconfig(args.config, new_kubeconfig, dry_run);
        }
        Commands::Delete {
            context,
            dry_run,
            yes,
        } => {
            let new_kubeconfig = delete_context(kubeconfig, context, dry_run || yes);

            write_kubeconfig(args.config, new_kubeconfig, dry_run);
        }
    }

    // for _ in 0..args.count {
    //     println!("Hello {}!", args.name);
    // }
}
