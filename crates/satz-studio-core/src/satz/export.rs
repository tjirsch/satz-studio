//! The two documents a customer receives, as satz writes them: `satz questions <estate>
//! --format <format> --out <file>`. The decisions sheet is `markdown` (or `pdf`, the same
//! sheet typeset by satz) and the workbook is `xlsx`; the formats themselves are satz's,
//! read from `satz questions --help` at runtime ([`formats`]), so the app offers exactly
//! the set the installed satz offers and never a rendering satz cannot produce.

use std::path::{Path, PathBuf};

use super::{SatzCli, SatzError};

/// One value `satz questions --format` takes, with the line satz's help gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionsFormat {
    /// what goes after `--format`: `markdown`, `xlsx`
    pub name: String,
    /// satz's own description of the value, when its help carries one
    pub description: Option<String>,
}

/// The formats `satz questions` offers, in the order its help lists them, read from the
/// long help of the installed satz.
pub async fn formats(cli: &SatzCli) -> Result<Vec<QuestionsFormat>, SatzError> {
    let help = cli.help(&["questions"]).await?;
    parse_formats(&help).map_err(|e| SatzError::Help {
        command: "questions --help".to_string(),
        reason: e,
    })
}

/// The possible values of `--format` in a clap long help: the `Possible values:` block
/// under the `--format <FORMAT>` option, one `- name: description` or `- name` line per
/// value, a description wrapped onto further lines joined back into one. A help without
/// that block, or with an empty one, is an error naming what is missing — never an
/// empty list.
pub fn parse_formats(help: &str) -> Result<Vec<QuestionsFormat>, String> {
    let mut lines = help.lines();
    lines
        .by_ref()
        .find(|l| l.trim_start().starts_with("--format <"))
        .ok_or("the help names no `--format` option")?;
    lines
        .by_ref()
        .take_while(|l| !l.trim_start().starts_with("--"))
        .find(|l| l.trim() == "Possible values:")
        .ok_or("the help lists no possible values under `--format`")?;
    let mut out: Vec<QuestionsFormat> = Vec::new();
    for line in lines {
        let t = line.trim();
        if t.is_empty() {
            break;
        }
        if let Some(value) = t.strip_prefix("- ") {
            let (name, description) = match value.split_once(':') {
                Some((name, d)) => (name.trim(), Some(d.trim().to_string())),
                None => (value.trim(), None),
            };
            out.push(QuestionsFormat {
                name: name.to_string(),
                description: description.filter(|d| !d.is_empty()),
            });
        } else {
            let last = out
                .last_mut()
                .ok_or_else(|| format!("`{t}` stands before the first possible value"))?;
            let d = last.description.get_or_insert_with(String::new);
            if !d.is_empty() {
                d.push(' ');
            }
            d.push_str(t);
        }
    }
    if out.is_empty() {
        return Err("the `--format` block lists no value".to_string());
    }
    Ok(out)
}

/// The file extension of a `--format` value. satz adds its own when the destination has
/// none; the app always names one, so the file it opens afterwards is the file satz wrote.
/// A format this table does not know takes its own name as the extension.
pub fn extension(format: &str) -> &str {
    match format {
        "text" => "txt",
        "markdown" => "md",
        other => other,
    }
}

/// The destination the export passes to `--out`: the chosen path, with the format's
/// extension added when the name carries none.
pub fn destination(chosen: &Path, format: &str) -> PathBuf {
    if chosen.extension().is_some() {
        chosen.to_path_buf()
    } else {
        chosen.with_extension(extension(format))
    }
}

/// The file name the save dialog proposes: the estate file's stem, then what the document
/// is, then the format's extension — `C0example-decisions.md`, `C0example-decisions.xlsx`.
pub fn proposed_name(estate: &str, format: &str) -> String {
    let stem = Path::new(estate)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| estate.to_string());
    format!("{stem}-decisions.{}", extension(format))
}

/// The argument vector after `satz --config <dir>`: `questions <estate> --format
/// <format> --out <out>` — the CLI's own two flags and nothing the app adds.
pub fn args(estate: &str, format: &str, out: &Path) -> Vec<String> {
    vec![
        "questions".to_string(),
        estate.to_string(),
        "--format".to_string(),
        format.to_string(),
        "--out".to_string(),
        out.display().to_string(),
    ]
}

/// What a run that exited zero left at `out`: its size. No file, or an empty one, is a
/// failure naming the path — an exit status is not proof that the document exists.
pub fn written(out: &Path) -> Result<u64, String> {
    let meta = std::fs::metadata(out).map_err(|e| {
        format!(
            "satz exited cleanly but {} is not there: {e}",
            out.display()
        )
    })?;
    match meta.len() {
        0 => Err(format!(
            "satz exited cleanly but wrote nothing into {}",
            out.display()
        )),
        n => Ok(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// satz 0.73.1's long help for `questions`, as it prints it.
    const HELP: &str = "\
satz v0.73.1
What this estate can be asked

Usage: satz questions [OPTIONS] --format <FORMAT> --out <FILE> <INPUT>

Arguments:
  <INPUT>
          Estate file (.satz, inside yaml_dir if relative)

Options:
      --format <FORMAT>
          Output format — markdown and pdf are the decisions sheet a human reads before an
          organisation is touched

          Possible values:
          - text:     human-readable terminal output
          - markdown
          - pdf:      the markdown typeset by satz itself — no tool on PATH, nothing to install
          - json
          - xlsx:     a workbook: the catalog a customer fills in and sends back

      --out <FILE>
          Where it goes

  -h, --help
          Print help (see a summary with '-h')
";

    #[test]
    fn the_possible_values_are_read_in_satz_s_order_with_satz_s_words() {
        let formats = parse_formats(HELP).unwrap();
        let names: Vec<&str> = formats.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["text", "markdown", "pdf", "json", "xlsx"]);
        assert_eq!(formats[1].description, None);
        assert_eq!(
            formats[4].description.as_deref(),
            Some("a workbook: the catalog a customer fills in and sends back")
        );
    }

    #[test]
    fn a_wrapped_description_is_joined_back_into_one() {
        let help = HELP.replace(
            "- pdf:      the markdown typeset by satz itself — no tool on PATH, nothing to install",
            "- pdf:      the markdown typeset by satz itself — no tool on\n            PATH, nothing to install",
        );
        let formats = parse_formats(&help).unwrap();
        assert_eq!(
            formats[2].description.as_deref(),
            Some("the markdown typeset by satz itself — no tool on PATH, nothing to install")
        );
    }

    #[test]
    fn a_help_without_the_block_is_an_error_and_never_an_empty_list() {
        assert!(parse_formats("Usage: satz questions").is_err());
        let no_values = HELP
            .split("          Possible values:")
            .next()
            .unwrap()
            .to_string()
            + "\n      --out <FILE>\n";
        assert!(parse_formats(&no_values).is_err());
        let empty = HELP.replace(
            "          - text:     human-readable terminal output\n          - markdown\n          - pdf:      the markdown typeset by satz itself — no tool on PATH, nothing to install\n          - json\n          - xlsx:     a workbook: the catalog a customer fills in and sends back\n",
            "",
        );
        assert!(parse_formats(&empty).is_err());
    }

    #[test]
    fn the_export_is_the_cli_s_two_flags_per_format() {
        for (format, file) in [
            ("markdown", "/tmp/out/C0example-decisions.md"),
            ("pdf", "/tmp/out/C0example-decisions.pdf"),
            ("xlsx", "/tmp/out/C0example-decisions.xlsx"),
            ("json", "/tmp/out/C0example-decisions.json"),
            ("text", "/tmp/out/C0example-decisions.txt"),
        ] {
            assert_eq!(
                args("C0example.satz", format, Path::new(file)),
                [
                    "questions",
                    "C0example.satz",
                    "--format",
                    format,
                    "--out",
                    file
                ]
            );
        }
    }

    #[test]
    fn the_destination_always_names_an_extension() {
        assert_eq!(
            destination(Path::new("/tmp/sheet"), "markdown"),
            Path::new("/tmp/sheet.md")
        );
        assert_eq!(
            destination(Path::new("/tmp/sheet"), "xlsx"),
            Path::new("/tmp/sheet.xlsx")
        );
        // a name the operator gave an extension keeps it
        assert_eq!(
            destination(Path::new("/tmp/sheet.markdown"), "markdown"),
            Path::new("/tmp/sheet.markdown")
        );
        assert_eq!(
            proposed_name("C0example.satz", "markdown"),
            "C0example-decisions.md"
        );
        assert_eq!(
            proposed_name("C0example.satz", "xlsx"),
            "C0example-decisions.xlsx"
        );
    }

    #[test]
    fn a_missing_or_empty_file_is_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("sheet.md");
        assert!(written(&out).unwrap_err().contains("is not there"));
        std::fs::write(&out, "").unwrap();
        assert!(written(&out).unwrap_err().contains("wrote nothing"));
        std::fs::write(&out, "# Decisions\n").unwrap();
        assert_eq!(written(&out).unwrap(), 12);
    }
}
