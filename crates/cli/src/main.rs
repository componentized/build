//! The cli of the componentized build, updates a repository's dependencies on the interfaces and
//! components of other repositories, see README.md.

mod complete;
mod id;
mod registry;
mod repo;
mod update;

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::builder::styling::{AnsiColor, Style};
use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use clap_complete::engine::{ArgValueCandidates, ArgValueCompleter};
use clap_complete::env::{CompleteEnv, Shells};
use semver::Version;

use crate::id::Id;
use crate::repo::Repo;

const BIN: &str = env!("CARGO_BIN_NAME");

const HEADER: Style = AnsiColor::Green.on_default().bold();
const ERROR: Style = AnsiColor::Red.on_default().bold();
const NOTE: Style = AnsiColor::Yellow.on_default();
const DIM: Style = Style::new().dimmed();

#[derive(Parser)]
#[command(
    name = BIN,
    version,
    about = "Update a componentized repository's dependencies on the interfaces and components of other repositories",
    long_about = "Update a componentized repository's dependencies on the interfaces and components of other \
        repositories.\n\
        \n\
        Run it in a repository synced with the componentized build, https://github.com/componentized/build, \
        or in a directory of one, the repository is the nearest directory with build/build.mk.",
    after_help = "Examples:\n  \
        List the dependencies\n    \
        componentized-build deps\n  \
        Update componentized:secrets, its interface and components, to 0.1.1\n    \
        componentized-build dep componentized:secrets 0.1.1\n  \
        Update it to the latest build of the version it's at, e.g. 0.1.1-dev\n    \
        componentized-build dep componentized:secrets\n  \
        Update its gate component alone\n    \
        componentized-build dep componentized:secrets-gate 0.1.1",
    propagate_version = true
)]
struct Cli {
    /// Run in the repository holding DIR, rather than the current directory
    #[arg(short = 'C', long = "directory", global = true, value_name = "DIR", value_hint = ValueHint::DirPath)]
    directory: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Update a dependency on another repository's interface and components to a version
    #[command(
        long_about = "Update a dependency on another repository's interface and components to a version, \
            everything in the repository that names them:\n\
            \n  \
            - the interface, a WIT package, wherever it's named at a version, \
            <namespace>:<name>/<interface>@<version>, in WIT, WAC, Rust or Markdown, and its entry in each \
            wkg.lock, pinned to the digest of the version published\n  \
            - each component pulled from the repository with wkg, components/dep-*/*.wkg, pinned to the \
            digests of the release and debug builds published, and wherever else its image is named, e.g. \
            its README\n\
            \n\
            The version is looked up in the registry before anything changes. A dev version is published \
            again with each build, updating to it again, or leaving out the version, pins the latest build.\n\
            \n\
            Files git ignores are left alone, as are the build synced into the repository, build/, and the \
            WIT fetched into wit/deps, `make` fetches the WIT again and pulls the components.",
        after_help = "Examples:\n  \
            Update componentized:secrets, its interface and components, to 0.1.1\n    \
            componentized-build dep componentized:secrets 0.1.1\n  \
            Update it to the latest build of the version it's at, e.g. 0.1.1-dev\n    \
            componentized-build dep componentized:secrets\n  \
            Update its gate component alone\n    \
            componentized-build dep componentized:secrets-gate 0.1.1\n  \
            Show what would change, without changing anything\n    \
            componentized-build dep componentized:secrets 0.1.1 --dry-run"
    )]
    Dep {
        /// The dependency, a WIT package, e.g. componentized:secrets, its interface and the components
        /// pulled from its repository, or one of the components, e.g. componentized:secrets-gate
        #[arg(value_name = "ID", value_parser = parse_id, add = ArgValueCandidates::new(complete::ids))]
        id: Id,

        /// The version to update to, e.g. 0.1.1 or 0.1.1-dev [default: the version it's at, pinned to
        /// its latest build]
        #[arg(value_name = "VERSION", value_parser = parse_version, add = ArgValueCompleter::new(complete::versions))]
        to: Option<Version>,

        /// Show what would change, without changing anything
        #[arg(short = 'n', long)]
        dry_run: bool,
    },

    /// List the dependencies on other repositories, the interfaces and components, by version
    #[command(
        long_about = "List the dependencies on other repositories, by version: the WIT packages named, other \
            than the repository's own, and the components pulled with wkg, components/dep-*/*.wkg."
    )]
    Deps,

    /// Print the script that registers the completions with a shell
    #[command(
        long_about = "Print the script that registers the completions with a shell, which completes the \
            commands, the dependencies of the repository and the versions published.",
        after_help = "Register them by adding the script to the shell's startup:\n  \
            bash        echo 'source <(componentized-build completions bash)' >> ~/.bashrc\n  \
            zsh         echo 'source <(componentized-build completions zsh)' >> ~/.zshrc\n  \
            fish        echo 'componentized-build completions fish | source' >> ~/.config/fish/config.fish\n  \
            elvish      echo 'eval (componentized-build completions elvish | slurp)' >> ~/.config/elvish/rc.elv\n  \
            powershell  'componentized-build completions powershell | Out-String | Invoke-Expression' >> $PROFILE"
    )]
    Completions {
        /// The shell [default: the shell of $SHELL]
        #[arg(value_parser = Shells::builtins().names().collect::<Vec<_>>())]
        shell: Option<String>,
    },
}

fn parse_id(s: &str) -> Result<Id, String> {
    s.parse()
}

fn parse_version(s: &str) -> Result<Version, String> {
    id::parse_version(s)
}

fn main() -> ExitCode {
    CompleteEnv::with_factory(Cli::command).bin(BIN).complete();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            anstream::eprintln!("{ERROR}error:{ERROR:#} {err}");
            for cause in err.chain().skip(1) {
                anstream::eprintln!("  {DIM}caused by:{DIM:#} {cause}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let directory = || match &cli.directory {
        Some(directory) => Ok(directory.clone()),
        None => std::env::current_dir().context("couldn't read the current directory"),
    };
    match cli.command {
        Command::Dep { id, to, dry_run } => {
            let repo = Repo::open(&repo::find_root(&directory()?)?)?;
            dep(&repo, &id, to.as_ref(), dry_run)
        }
        Command::Deps => {
            let repo = Repo::open(&repo::find_root(&directory()?)?)?;
            deps(&repo)
        }
        Command::Completions { shell } => completions(shell),
    }
}

fn dep(repo: &Repo, id: &Id, version: Option<&Version>, dry_run: bool) -> Result<()> {
    let registry = registry::Registries::new(&repo.root, Duration::from_secs(30))?;
    let plan = update::plan(repo, &registry, id, version)?;

    let from: Vec<String> = plan.from.iter().map(Version::to_string).collect();
    if plan.from.len() == 1 && plan.from.contains(&plan.to) {
        anstream::println!("{HEADER}Checking{HEADER:#} {} {}, for its latest build", plan.id, plan.to);
    } else {
        anstream::println!("{HEADER}Updating{HEADER:#} {} {} → {}", plan.id, from.join(", "), plan.to);
    }
    let width = plan.resolved.iter().map(|r| r.what.len()).max().unwrap_or(0);
    for resolved in &plan.resolved {
        anstream::println!(
            "  {:width$}  {} {DIM}{}{DIM:#}",
            resolved.what,
            resolved.reference,
            update::short(&resolved.digest)
        );
        if let Some(note) = &resolved.note {
            anstream::println!("  {:width$}  {NOTE}{note}{NOTE:#}", "");
        }
    }

    if plan.edits.is_empty() {
        anstream::println!("{HEADER}Up to date{HEADER:#}, nothing changed");
        return Ok(());
    }
    if dry_run {
        anstream::println!("{HEADER}Would update{HEADER:#} {}, nothing changed as it's a dry run", files(plan.edits.len()));
    } else {
        plan.apply(repo)?;
        anstream::println!("{HEADER}Updated{HEADER:#} {}", files(plan.edits.len()));
    }
    for edit in &plan.edits {
        anstream::println!("  {}", edit.path.display());
        for change in &edit.changes {
            anstream::println!("    {DIM}{change}{DIM:#}");
        }
    }
    if !dry_run {
        anstream::println!(
            "\nRun `make` to fetch the WIT and pull the components it now pins, and build with them."
        );
    }
    Ok(())
}

fn files(count: usize) -> String {
    match count {
        1 => "1 file".into(),
        count => format!("{count} files"),
    }
}

fn deps(repo: &Repo) -> Result<()> {
    let dependencies = repo.dependencies();
    if dependencies.is_empty() {
        println!("{} has no dependencies on other repositories", repo.root.display());
        return Ok(());
    }
    let rows: Vec<(String, String, String)> = dependencies
        .values()
        .map(|dependency| {
            let versions: Vec<String> = dependency.versions().iter().map(Version::to_string).collect();
            let mut what = Vec::new();
            if !dependency.interface.is_empty() {
                what.push("interface".to_string());
            }
            if !dependency.components.is_empty() {
                let several = dependency.versions().len() > 1;
                let components: Vec<String> = dependency
                    .components
                    .iter()
                    .map(|(component, versions)| match several {
                        true => format!(
                            "{component}@{}",
                            versions.iter().map(Version::to_string).collect::<Vec<_>>().join(",")
                        ),
                        false => component.clone(),
                    })
                    .collect();
                what.push(format!("components {}", components.join(", ")));
            }
            (dependency.id.to_string(), versions.join(", "), what.join(", "))
        })
        .collect();
    let id_width = rows.iter().map(|r| r.0.len()).max().unwrap_or(0);
    let version_width = rows.iter().map(|r| r.1.len()).max().unwrap_or(0);
    for (id, versions, what) in rows {
        anstream::println!("{HEADER}{id:id_width$}{HEADER:#}  {versions:version_width$}  {DIM}{what}{DIM:#}");
    }
    Ok(())
}

fn completions(shell: Option<String>) -> Result<()> {
    let shells = Shells::builtins();
    let shell = match shell {
        Some(shell) => shell,
        None => {
            let path = std::env::var("SHELL").map_err(|_| {
                anyhow::anyhow!("name the shell, $SHELL isn't set, one of {}", shells.names().collect::<Vec<_>>().join(", "))
            })?;
            std::path::Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or(path)
        }
    };
    let Some(completer) = shells.completer(&shell) else {
        bail!(
            "there are no completions for {shell}, name one of {}",
            shells.names().collect::<Vec<_>>().join(", ")
        );
    };
    // the binary run for completions, this one, wherever it's installed
    let exe = std::env::current_exe()
        .ok()
        .map(|exe| exe.to_string_lossy().into_owned())
        .unwrap_or_else(|| BIN.to_string());
    let mut stdout = std::io::stdout().lock();
    completer.write_registration("COMPLETE", BIN, BIN, &exe, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_the_cli() {
        Cli::command().debug_assert();
    }

    #[test]
    fn validates_the_arguments() {
        let err = Cli::try_parse_from([BIN, "dep", "Componentized:secrets"]).err().unwrap().to_string();
        assert!(err.contains("may only have lowercase letters"), "{err}");
        let err = Cli::try_parse_from([BIN, "dep", "componentized:secrets", "latest"]).err().unwrap().to_string();
        assert!(err.contains("not a semantic version"), "{err}");
        assert!(Cli::try_parse_from([BIN, "dep", "componentized:secrets", "v0.1.1", "-n"]).is_ok());
    }
}
