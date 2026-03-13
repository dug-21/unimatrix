use super::*;
use clap::CommandFactory;

#[test]
fn test_binary_name_is_unimatrix() {
    assert_eq!(Cli::command().get_name(), "unimatrix");
}

#[test]
fn test_no_subcommand_defaults_to_server_mode() {
    let cli = Cli::try_parse_from(["unimatrix"]).unwrap();
    assert!(cli.command.is_none());
}

#[test]
fn test_hook_subcommand_parsed() {
    let cli = Cli::try_parse_from(["unimatrix", "hook", "SessionStart"]).unwrap();
    match cli.command {
        Some(Command::Hook { event }) => assert_eq!(event, "SessionStart"),
        other => panic!("expected Hook, got {other:?}"),
    }
}

#[test]
fn test_version_subcommand_parsed() {
    let cli = Cli::try_parse_from(["unimatrix", "version"]).unwrap();
    assert!(matches!(cli.command, Some(Command::Version)));
}

#[test]
fn test_model_download_subcommand_parsed() {
    let cli = Cli::try_parse_from(["unimatrix", "model-download"]).unwrap();
    assert!(matches!(cli.command, Some(Command::ModelDownload)));
}

#[test]
fn test_export_subcommand_unchanged() {
    let cli = Cli::try_parse_from(["unimatrix", "export", "--output", "/tmp/out.json"]).unwrap();
    match cli.command {
        Some(Command::Export { output }) => {
            assert_eq!(output, Some(PathBuf::from("/tmp/out.json")));
        }
        other => panic!("expected Export, got {other:?}"),
    }
}

#[test]
fn test_import_subcommand_unchanged() {
    let cli = Cli::try_parse_from(["unimatrix", "import", "--input", "/tmp/in.json"]).unwrap();
    match cli.command {
        Some(Command::Import { input, .. }) => {
            assert_eq!(input, PathBuf::from("/tmp/in.json"));
        }
        other => panic!("expected Import, got {other:?}"),
    }
}

#[test]
fn test_project_dir_flag_accepted() {
    let cli = Cli::try_parse_from(["unimatrix", "--project-dir", "/some/path", "version"]).unwrap();
    assert_eq!(cli.project_dir, Some(PathBuf::from("/some/path")));
    assert!(matches!(cli.command, Some(Command::Version)));
}

#[test]
fn test_verbose_flag_accepted() {
    let cli = Cli::try_parse_from(["unimatrix", "-v", "version"]).unwrap();
    assert!(cli.verbose);
}

#[test]
fn test_handle_version_prints_version() {
    // handle_version(None) just prints to stdout; verify it returns Ok
    let result = handle_version(None);
    assert!(result.is_ok());
}
