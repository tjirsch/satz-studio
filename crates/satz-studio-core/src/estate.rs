//! An estate directory as satz sees it: `config.toml` anchors it, `yaml_dir` holds the
//! `.satz` files, `include_dirs` are where `use "…"` resolves, `presets_dir` the pack
//! library, `schema_dir` the provider schema. The defaults, the path resolution and the
//! loader mirror satz (`src/main.rs`: `ToolConfig`, `resolved_config`, `estate_path`,
//! the loader in `pipeline_b_generate`, and `find_configs` / `declares_an_estate` in
//! `src/mcp.rs`), so the app opens exactly what `satz --config <dir>` would.

use std::path::{Path, PathBuf};

/// The keys of `config.toml` the app reads. Unknown keys are satz's business and pass
/// through untouched; the app never writes this file.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ToolConfig {
    #[serde(default = "default_yaml_dir")]
    pub yaml_dir: String,
    #[serde(default = "default_hcl_dir")]
    pub hcl_dir: String,
    #[serde(default = "default_include_dirs")]
    pub include_dirs: Vec<String>,
    #[serde(default = "default_schema_dir")]
    pub schema_dir: String,
    #[serde(default = "default_presets_dir")]
    pub presets_dir: String,
    #[serde(default = "default_tf_tool")]
    pub tf_tool: String,
    #[serde(default = "default_provider_version")]
    pub provider_version: String,
    #[serde(default = "default_validation_level")]
    pub validation_level: String,
    #[serde(default)]
    pub import_config: Option<String>,
}

fn default_yaml_dir() -> String {
    "satz".to_string()
}
fn default_hcl_dir() -> String {
    "hcl".to_string()
}
fn default_include_dirs() -> Vec<String> {
    vec![".".to_string(), default_yaml_dir()]
}
fn default_schema_dir() -> String {
    "schemas".to_string()
}
fn default_presets_dir() -> String {
    "presets".to_string()
}
fn default_tf_tool() -> String {
    "tofu".to_string()
}
fn default_provider_version() -> String {
    "7.14.1".to_string()
}
fn default_validation_level() -> String {
    "warn".to_string()
}

impl ToolConfig {
    /// The same config with every relative path resolved against the config's own
    /// directory — what makes a command runnable from anywhere.
    pub fn resolved(&self, config_dir: &Path) -> ToolConfig {
        let at = |d: &str| -> String {
            if Path::new(d).is_relative() {
                config_dir.join(d).to_string_lossy().into_owned()
            } else {
                d.to_string()
            }
        };
        ToolConfig {
            yaml_dir: at(&self.yaml_dir),
            hcl_dir: at(&self.hcl_dir),
            include_dirs: self.include_dirs.iter().map(|d| at(d)).collect(),
            schema_dir: at(&self.schema_dir),
            presets_dir: at(&self.presets_dir),
            tf_tool: self.tf_tool.clone(),
            provider_version: self.provider_version.clone(),
            validation_level: self.validation_level.clone(),
            import_config: self.import_config.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EstateError {
    #[error(
        "{0}: no config.toml here (an estate is the directory holding one, or the file itself)"
    )]
    NoConfig(PathBuf),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: not a satz config.toml: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// One estate directory: the config as written, and the same config resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct EstateDir {
    pub config_path: PathBuf,
    pub dir: PathBuf,
    pub tool: ToolConfig,
    pub runtime: ToolConfig,
}

impl EstateDir {
    /// Open an estate by its `config.toml` or by the directory holding it.
    pub fn open(config_or_dir: &Path) -> Result<Self, EstateError> {
        let config_path = if config_or_dir.is_dir() {
            config_or_dir.join("config.toml")
        } else {
            config_or_dir.to_path_buf()
        };
        if !config_path.is_file() {
            return Err(EstateError::NoConfig(config_or_dir.to_path_buf()));
        }
        let text = std::fs::read_to_string(&config_path).map_err(|e| EstateError::Io {
            path: config_path.clone(),
            source: e,
        })?;
        let tool: ToolConfig = toml::from_str(&text).map_err(|e| EstateError::Parse {
            path: config_path.clone(),
            source: e,
        })?;
        let dir = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let runtime = tool.resolved(&dir);
        Ok(Self {
            config_path,
            dir,
            tool,
            runtime,
        })
    }

    /// Every `config.toml` under `root`, depth-limited and blind to the directories that
    /// never hold one (`hcl/`, `target/`, `evidence/`, `node_modules/`, dot-directories),
    /// at most 200 — a fleet root is somebody's home directory in the worst case, as
    /// satz's own `find_configs` says.
    pub fn discover(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        find_configs(root, 6, &mut out);
        out
    }

    /// The `.satz` files in `yaml_dir` that declare an estate (a pack or a fragment is
    /// not one), sorted by name.
    pub fn estates(&self) -> Result<Vec<PathBuf>, EstateError> {
        let yaml_dir = PathBuf::from(&self.runtime.yaml_dir);
        let entries = std::fs::read_dir(&yaml_dir).map_err(|e| EstateError::Io {
            path: yaml_dir.clone(),
            source: e,
        })?;
        let mut out = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|e| EstateError::Io {
                    path: yaml_dir.clone(),
                    source: e,
                })?
                .path();
            if path.extension().and_then(|e| e.to_str()) != Some("satz") || is_checked_temp(&path) {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| EstateError::Io {
                path: path.clone(),
                source: e,
            })?;
            if declares_an_estate(&text) {
                out.push(path);
            }
        }
        out.sort();
        Ok(out)
    }

    /// Where a name given on the command line resolves, as `satz` resolves it: absolute
    /// as is, an existing relative path as is, otherwise inside `yaml_dir`.
    pub fn estate_path(&self, estate: &Path) -> PathBuf {
        if estate.is_absolute() || estate.exists() {
            return estate.to_path_buf();
        }
        PathBuf::from(&self.runtime.yaml_dir).join(estate)
    }

    /// The loader satz-core's pipeline calls for every `use "…"`: the using file's own
    /// directory first, then every `include_dirs` entry, first hit wins.
    pub fn loader(
        &self,
        main: &Path,
    ) -> impl Fn(&str) -> Result<String, String> + Send + Sync + 'static {
        let base_dir = main.parent().unwrap_or(Path::new(".")).to_path_buf();
        let include_dirs: Vec<PathBuf> = self
            .runtime
            .include_dirs
            .iter()
            .map(PathBuf::from)
            .collect();
        move |p: &str| -> Result<String, String> {
            let mut candidates = vec![base_dir.join(p)];
            candidates.extend(include_dirs.iter().map(|d| d.join(p)));
            for c in candidates {
                if c.exists() {
                    return std::fs::read_to_string(&c).map_err(|e| e.to_string());
                }
            }
            Err(format!("use \"{}\": file not found", p))
        }
    }

    /// The estate's resolved params, schema-free (as `satz questions` reads them).
    pub fn params(
        &self,
        main: &Path,
    ) -> Result<satz_core::pipeline::Env, satz_core::pipeline::PipelineError> {
        let src =
            std::fs::read_to_string(main).map_err(|e| satz_core::pipeline::PipelineError {
                file: main.to_string_lossy().into_owned(),
                line: 0,
                msg: e.to_string(),
            })?;
        satz_core::pipeline::estate_params(&main.to_string_lossy(), &src, &self.loader(main))
    }

    /// Whether the estate has acknowledged the notice on `param`, by satz's own rule
    /// over the resolved params of [`EstateDir::params`]: the param is bound `true`,
    /// and nothing else counts. The app asks this rather than reading the param
    /// itself, so a window and satz never disagree about what is still open.
    pub fn acknowledged(env: &satz_core::pipeline::Env, param: &str) -> bool {
        satz_core::pipeline::acknowledged(env, param)
    }

    /// `deployment_mode` as the estate binds it — `local` or `cloud` — or `None` when
    /// it binds nothing (a skeleton before its first answers).
    pub fn deployment_mode(
        &self,
        main: &Path,
    ) -> Result<Option<String>, satz_core::pipeline::PipelineError> {
        Ok(self
            .params(main)?
            .get("deployment_mode")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    pub fn schema_dir(&self) -> PathBuf {
        PathBuf::from(&self.runtime.schema_dir)
    }
    pub fn presets_dir(&self) -> PathBuf {
        PathBuf::from(&self.runtime.presets_dir)
    }
    pub fn hcl_dir(&self) -> PathBuf {
        PathBuf::from(&self.runtime.hcl_dir)
    }
    pub fn yaml_dir(&self) -> PathBuf {
        PathBuf::from(&self.runtime.yaml_dir)
    }
}

/// How far the generated HCL directory has been taken, read from disk and nothing run.
/// Two facts, and only what each proves: `main.tf` is there, so the estate has been
/// transpiled here; `.terraform` is there, so the tool's init has run. A plan needs an
/// initialised directory, so `initialised == false` proves no plan has run against this
/// checkout — and `true` proves nothing about a plan, which leaves no trace of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HclState {
    /// `hcl_dir/main.tf` exists
    pub transpiled: bool,
    /// `hcl_dir/.terraform` exists
    pub initialised: bool,
}

impl HclState {
    pub fn read(hcl_dir: &Path) -> HclState {
        HclState {
            transpiled: hcl_dir.join("main.tf").is_file(),
            initialised: hcl_dir.join(".terraform").is_dir(),
        }
    }
}

/// A value edit in flight: the file `edit` writes beside the real one for satz to check
/// (`<stem>.studio-tmp.satz`). It is a copy of an estate and never an estate of its own.
pub fn is_checked_temp(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".studio-tmp.satz"))
}

/// Whether a `.satz` file is an ESTATE rather than a pack or a fragment: the statement
/// is the definition, so read for it instead of guessing from the name.
pub fn declares_an_estate(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with("estate ") || l == "estate"
    })
}

fn find_configs(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 || out.len() >= 200 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut dirs = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        if path.is_dir() {
            if !matches!(
                name.as_str(),
                "hcl" | "target" | "evidence" | "node_modules"
            ) {
                dirs.push(path);
            }
        } else if name == "config.toml" {
            out.push(path);
        }
    }
    dirs.sort();
    for d in dirs {
        find_configs(&d, depth - 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/smoke")
    }

    #[test]
    fn the_fixture_opens_by_directory_and_by_file() {
        let a = EstateDir::open(&fixture()).unwrap();
        let b = EstateDir::open(&fixture().join("config.toml")).unwrap();
        assert_eq!(a, b);
        assert!(
            a.runtime.yaml_dir.ends_with("vendor/satz/tests/smoke/yaml"),
            "{}",
            a.runtime.yaml_dir
        );
        assert!(
            Path::new(&a.runtime.presets_dir)
                .join("estate-map.satz")
                .is_file()
        );
    }

    #[test]
    fn the_smoke_directory_holds_three_estates_and_three_packs() {
        let e = EstateDir::open(&fixture()).unwrap();
        let names: Vec<String> = e
            .estates()
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["greenfield.satz", "showcase.satz", "smoke.satz"]);
    }

    #[test]
    fn the_loader_resolves_a_pack_through_include_dirs() {
        let e = EstateDir::open(&fixture()).unwrap();
        let main = e.yaml_dir().join("smoke.satz");
        let load = e.loader(&main);
        assert!(
            load("presets/estate-core.satz")
                .unwrap()
                .contains("pack estate_core")
        );
        assert!(
            load("presets/none.satz")
                .unwrap_err()
                .contains("file not found")
        );
    }

    #[test]
    fn params_and_deployment_mode_read_without_a_schema() {
        let e = EstateDir::open(&fixture()).unwrap();
        let main = e.yaml_dir().join("smoke.satz");
        let env = e.params(&main).unwrap();
        assert!(env.contains_key("customer_shortname"));
        assert!(e.deployment_mode(&main).unwrap().is_some());
    }

    #[test]
    fn discover_finds_the_fixture_and_skips_output_directories() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
        let found = EstateDir::discover(&root);
        assert_eq!(found, vec![fixture().join("config.toml")]);
    }

    /// The first string literal in the body of `fn <name>()` in satz's
    /// `src/settings.rs` at the pinned submodule — the default satz itself applies.
    fn satz_default(name: &str) -> String {
        let src = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/satz/src/settings.rs"),
        )
        .unwrap();
        let at = src
            .find(&format!("fn {name}()"))
            .unwrap_or_else(|| panic!("satz's settings.rs has no `fn {name}()`"));
        let rest = &src[at..];
        let open = rest.find('"').unwrap() + 1;
        let close = open + rest[open..].find('"').unwrap();
        rest[open..close].to_string()
    }

    #[test]
    fn the_defaults_for_omitted_keys_are_satzs() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "").unwrap();
        let e = EstateDir::open(tmp.path()).unwrap();
        let c = &e.tool;
        assert_eq!(c.yaml_dir, satz_default("default_yaml_dir"));
        assert_eq!(c.yaml_dir, "satz");
        assert_eq!(c.include_dirs, [".", "satz"]);
        assert_eq!(c.hcl_dir, satz_default("default_hcl_dir"));
        assert_eq!(c.schema_dir, satz_default("default_schema_dir"));
        assert_eq!(c.presets_dir, satz_default("default_presets_dir"));
        assert_eq!(c.tf_tool, satz_default("default_tf_tool"));
        assert_eq!(c.provider_version, satz_default("default_version"));
        assert_eq!(c.validation_level, satz_default("default_validation_level"));
        assert!(e.yaml_dir().ends_with("satz"));
    }

    #[test]
    fn a_directory_without_config_is_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            EstateDir::open(dir.path()),
            Err(EstateError::NoConfig(_))
        ));
    }

    #[test]
    fn a_checked_temp_file_is_not_listed_as_an_estate() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml = tmp.path().join("yaml");
        std::fs::create_dir_all(&yaml).unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
        std::fs::write(yaml.join("acme.satz"), "estate acme\n").unwrap();
        std::fs::write(yaml.join("acme.studio-tmp.satz"), "estate acme\n").unwrap();
        let e = EstateDir::open(tmp.path()).unwrap();
        let names: Vec<String> = e
            .estates()
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["acme.satz"]);
        assert!(is_checked_temp(Path::new("/x/yaml/acme.studio-tmp.satz")));
    }

    #[test]
    fn declares_an_estate_reads_the_statement_not_the_name() {
        assert!(declares_an_estate("// a comment\nestate acme\n"));
        assert!(!declares_an_estate("pack acme version \"1.0\"\n"));
    }

    #[test]
    fn the_hcl_state_reads_the_two_files_that_prove_a_transpile_and_an_init() {
        let tmp = tempfile::tempdir().unwrap();
        let hcl = tmp.path().join("hcl");
        assert_eq!(HclState::read(&hcl), HclState::default());
        std::fs::create_dir_all(&hcl).unwrap();
        assert_eq!(HclState::read(&hcl), HclState::default());
        std::fs::write(hcl.join("main.tf"), "# generated\n").unwrap();
        assert_eq!(
            HclState::read(&hcl),
            HclState {
                transpiled: true,
                initialised: false
            }
        );
        std::fs::create_dir_all(hcl.join(".terraform")).unwrap();
        assert_eq!(
            HclState::read(&hcl),
            HclState {
                transpiled: true,
                initialised: true
            }
        );
    }
}
