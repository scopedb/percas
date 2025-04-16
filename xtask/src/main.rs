use std::fs;
use std::process::Command as StdCommand;

use clap::Parser;
use clap::Subcommand;
use percas_styled::styled;
use serde::Deserialize;

const CARGO_WORKSPACE_DIR: &str = env!("CARGO_WORKSPACE_DIR");

#[derive(Parser)]
#[clap(version, styles=styled())]
struct Command {
    #[clap(subcommand)]
    sub: SubCommand,
}

impl Command {
    fn run(self) {
        match self.sub {
            SubCommand::Build(cmd) => cmd.run(),
            SubCommand::Lint(cmd) => cmd.run(),
            SubCommand::Test(cmd) => cmd.run(),
        }
    }
}

#[derive(Subcommand)]
enum SubCommand {
    #[clap(about = "Compile workspace packages.")]
    Build(CommandBuild),
    #[clap(about = "Run format and clippy checks.")]
    Lint(CommandLint),
    #[clap(about = "Run unit tests.")]
    Test(CommandTest),
}

#[derive(Parser)]
struct CommandBuild {
    #[arg(long, help = "Assert that `Cargo.lock` will remain unchanged.")]
    locked: bool,
    #[arg(
        long,
        help = "Build all the tests, benches and examples in the workspace."
    )]
    all: bool,
}

impl CommandBuild {
    fn run(self) {
        run_command(make_build_cmd(self.locked, self.all));
    }
}

#[derive(Parser)]
struct CommandTest {
    #[arg(long, help = "Run tests serially and do not capture output.")]
    no_capture: bool,
}

impl CommandTest {
    fn run(self) {
        run_command(make_test_cmd(self.no_capture));
    }
}

#[derive(Parser)]
#[clap(name = "lint")]
struct CommandLint {
    #[arg(long, help = "Automatically apply lint suggestions.")]
    fix: bool,
}

impl CommandLint {
    fn run(self) {
        if self.fix {
            with_macro_normalized(|| run_command(make_format_cmd(true)));
            run_custom_format(true);
            run_command(make_sg_lint_cmd());
            run_command(make_taplo_cmd(true));
            // cannot fix; but still report errors because developers often call
            // 'cargo x lint --fix' only during developing
            run_command(make_typos_cmd());
            run_command(make_clippy_cmd(true));
            run_command(make_format_cmd(true));
        } else {
            run_custom_format(false);
            run_command(make_sg_lint_cmd());
            run_command(make_taplo_cmd(false));
            run_command(make_typos_cmd());
            run_command(make_format_cmd(false));
            run_command(make_clippy_cmd(false));
        }
    }
}

fn find_command(cmd: &str) -> StdCommand {
    let output = StdCommand::new("which")
        .arg(cmd)
        .output()
        .expect("broken command: which");
    if output.status.success() {
        let result = String::from_utf8_lossy(&output.stdout);
        let mut cmd = StdCommand::new(result.trim());
        cmd.current_dir(CARGO_WORKSPACE_DIR);
        cmd
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("{cmd} not found.\nstdout: {}\nstderr: {}", stdout, stderr);
    }
}

fn ensure_installed(bin: &str, crate_name: &str) {
    let output = StdCommand::new("which")
        .arg(bin)
        .output()
        .expect("broken command: which");
    if !output.status.success() {
        let mut cmd = find_command("cargo");
        cmd.args(["install", crate_name]);
        run_command(cmd);
    }
}

fn run_command(mut cmd: StdCommand) {
    println!("{cmd:?}");
    let status = cmd.status().expect("failed to execute process");
    assert!(status.success(), "command failed: {status}");
}

fn run_command_with_stdout(mut cmd: StdCommand) -> String {
    println!("{cmd:?}");
    let stdout = cmd.output().expect("failed to execute process").stdout;
    String::from_utf8(stdout).expect("failed to parse stdout")
}

fn make_build_cmd(locked: bool, all: bool) -> StdCommand {
    let mut cmd = find_command("cargo");
    cmd.args(["build", "--workspace", "--all-features"]);
    if all {
        cmd.args(["--bins", "--examples", "--tests", "--benches"]);
    }
    if locked {
        cmd.arg("--locked");
    }
    cmd
}

fn make_test_cmd(no_capture: bool) -> StdCommand {
    ensure_installed("cargo-nextest", "cargo-nextest");
    let mut cmd = find_command("cargo");
    cmd.args(["nextest", "run", "--workspace"]);
    if no_capture {
        cmd.arg("--no-capture");
    }
    cmd
}

fn make_format_cmd(fix: bool) -> StdCommand {
    let mut cmd = find_command("cargo");
    cmd.args(["fmt", "--all"]);
    if !fix {
        cmd.arg("--check");
    }
    cmd
}

fn make_clippy_cmd(fix: bool) -> StdCommand {
    let mut cmd = find_command("cargo");
    cmd.args([
        "clippy",
        "--tests",
        "--all-features",
        "--all-targets",
        "--workspace",
    ]);
    if fix {
        cmd.args(["--fix", "--allow-staged", "--allow-dirty"]);
    } else {
        cmd.args(["--", "-D", "warnings"]);
    }
    cmd
}

fn make_sg_lint_cmd() -> StdCommand {
    ensure_installed("ast-grep", "ast-grep");
    let mut cmd = find_command("ast-grep");
    cmd.args(["scan"]);
    cmd
}

fn make_sg_search_cmd(rule_file: &str) -> StdCommand {
    ensure_installed("ast-grep", "ast-grep");
    let mut cmd = find_command("ast-grep");
    cmd.args(["scan", "--rule", rule_file, "--json=pretty"]);
    cmd
}

fn make_sg_fix_cmd(rule_file: &str) -> StdCommand {
    ensure_installed("ast-grep", "ast-grep");
    let mut cmd = find_command("ast-grep");
    cmd.args(["scan", "--rule", rule_file, "--update-all"]);
    cmd
}

fn make_typos_cmd() -> StdCommand {
    ensure_installed("typos", "typos-cli");
    find_command("typos")
}

fn make_taplo_cmd(fix: bool) -> StdCommand {
    ensure_installed("taplo", "taplo-cli");
    let mut cmd = find_command("taplo");
    if fix {
        cmd.args(["format"]);
    } else {
        cmd.args(["format", "--check"]);
    }
    cmd
}

fn with_macro_normalized(f: impl FnOnce()) {
    run_command(make_sg_fix_cmd(&format!(
        "{CARGO_WORKSPACE_DIR}/.lints/utils/pre-format.yml"
    )));

    f();

    run_command(make_sg_fix_cmd(&format!(
        "{CARGO_WORKSPACE_DIR}/.lints/utils/post-format.yml"
    )));
}

/// Fix indentation of inline snapshots.
fn run_custom_format(fix: bool) {
    #[derive(Deserialize)]
    struct ScanResult {
        file: String,
        range: Range,
        lines: String,
        text: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Range {
        byte_offset: ByteOffset,
        start: TextPosition,
    }

    #[derive(Deserialize)]
    struct ByteOffset {
        start: usize,
        end: usize,
    }

    #[derive(Deserialize)]
    struct TextPosition {
        line: usize,
        column: usize,
    }

    let command = make_sg_search_cmd(&format!(
        "{CARGO_WORKSPACE_DIR}/.lints/utils/inline-snap.yml"
    ));
    let command_output = run_command_with_stdout(command);
    let inline_snaps: Vec<ScanResult> = serde_json::from_str(&command_output).unwrap();

    for snap in inline_snaps.iter().rev() {
        let file = format!("{CARGO_WORKSPACE_DIR}/{}", snap.file);
        let start = snap.range.byte_offset.start;
        let end = snap.range.byte_offset.end;
        let line_number = snap.range.start.line;
        let col_number = snap.range.start.column;
        let text = &snap.text;
        let indent = snap.lines.find('@').unwrap_or(0);
        let format_text = indent::indent_by(indent, unindent::unindent(text));

        if text != &format_text {
            if fix {
                let mut content = fs::read_to_string(&file).unwrap();
                content.replace_range(start..end, &format_text);
                fs::write(&file, content).unwrap();
            } else {
                pretty_assertions::assert_eq!(
                    text,
                    &format_text,
                    "diff in {}:{}:{}",
                    &file,
                    line_number + 1,
                    col_number + 1
                );
                std::process::exit(1);
            }
        }
    }
}

fn main() {
    let cmd = Command::parse();
    cmd.run()
}
