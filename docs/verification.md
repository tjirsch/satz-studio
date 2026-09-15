# satz-studio verification

What is proven, where, and how to run it. Every automated check runs offline against
`tests/fixtures` and the pinned `vendor/satz`; the ones that drive the `satz` binary
need it installed at `MIN_SATZ` or newer. The manual checks that need a window, a
credential or a machine are listed at the end.

## 1. What is proven where

The end-to-end tests (`crates/satz-studio-core/tests/e2e_*.rs`) take the paths the
app takes — `Snapshot::take`, `satz_interview` over the estate's `satz mcp`,
`Snapshot::verify` through `McpChecker`; `EditSession::apply` and `Proposed::commit`;
the map line uncommented by the rule of the app's `uncomment_line` — against a
temporary estate written by `satz interview --create` over `vendor/satz`, and against
a copy of satz's smoke estate. The unit tests of each module are named where they
carry a claim the harness relies on.

| claim | test file | test |
|---|---|---|
| The sixteen day-0 answers satz's smoke matrix pipes into `satz interview` (`vendor/satz/scripts/smoke.sh`), given one `satz_interview` call per typed value and then `accept_defaults`, end complete: `summary.complete`, sixteen answered, `written == 9` for the defaults and the two derived names, `rename_to == "C0example.satz"` | `tests/e2e_interview.rs` | `the_app_path_and_the_cli_path_end_in_the_same_estate` |
| A derived name is offered once its input is typed: `infra_project_name` is `blocking` until `customer_shortname` lands, then defaults to `acme-infra-001` | `tests/e2e_interview.rs` | same |
| The `questions` report `SatzCli::json_report` reads back carries the same questions and summary as the last `satz_interview` | `tests/e2e_interview.rs` | same |
| The CLI-driven interview on a second copy, fed the same piped input, writes the same `params` set and the same `use` line states; outside `params { }` the two files are byte-identical | `tests/e2e_interview.rs` | same |
| Enabling the map removes the `// ` of the exact line `// use "presets/estate-map.satz"` and nothing else; the check passes; the map's choices are open afterwards | `tests/e2e_map.rs` | `the_map_goes_in_and_a_choice_answered_twice_leaves_its_line_active` |
| `use_billing_permissions: true` through `satz_interview` binds the param and leaves `use "presets/billing-account-permissions.satz" when use_billing_permissions` active; the model row is `On` | `tests/e2e_map.rs` | same |
| `use_billing_permissions: false` leaves the line active and the param `false`; the model carries one `Note` "line active, gate false" on that line | `tests/e2e_map.rs` | same |
| A gate bound `true` whose line is gone: `ReplaceParam` appends it and the commit lands at satz's default validation level; the check prints the unadopted-pack warning, which `parse_satz_output` turns into a diagnostic naming the pack and `satz merge-presets`; the model has one `Absent` row, last, and no param row for the gate | `tests/e2e_map.rs` | `a_gate_bound_true_without_its_line_is_absent_and_the_check_names_the_pack` |
| At `validation_level = "error"` the same sentence is a refusal: `CliChecker` returns it as one error diagnostic of kind `unadopted-pack`, and a commit under that checker is `Rollback::Check` with it, the file's bytes kept, no temp file left | `tests/e2e_map.rs` | same |
| A `ReplaceValue` with the node's own value commits with the file's hash unchanged and the output of `satz transpile --check` identical before and after, the banner stripped | `tests/e2e_edit.rs` | `a_no_op_edit_leaves_the_hash_and_the_check_output_unchanged` |
| An edit on `showcase.satz`'s `audit_retention_days` line replaces the value span alone: the bytes before and after the span, the `=` column, the spaces before the comment and the comment itself are the original's; the reverse edit restores the original bytes and hash | `tests/e2e_edit.rs` | `an_edit_replaces_the_value_span_alone_and_the_reverse_edit_restores_the_bytes` |
| A good edit lands through the MCP checker; a bad edit rolls back with a diagnostic naming the real file and line, the original bytes intact, no `.studio-tmp.satz` left; a file changed on disk is refused; a delegated write is verified, and a broken one restored | `tests/edit_commit.rs` | `a_good_edit_lands_through_the_mcp_checker`, `a_bad_edit_rolls_back_naming_the_real_file`, `a_file_changed_on_disk_is_refused`, `a_delegated_write_is_verified_and_a_broken_one_is_restored` |
| `McpChecker` and `CliChecker` return the same verdict and, for a refusal, the same `(file, line, kind, message)` set, on six edits | `tests/edit_checkers_agree.rs` | `the_two_checkers_agree_on_six_cases` |
| A refusal at `validation_level = "error"` is the same findings through both checkers — the JSON over MCP and the `Debug` of the refusal on the CLI — with the `unadopted-pack` error among them naming the pack and `satz merge-presets` | `tests/edit_checkers_agree.rs` | `a_refusal_is_the_same_findings_through_both_checkers` |
| At satz's default level the same estate passes, and the `unadopted-pack` warning is in the MCP summary's `findings`; the CLI summary carries none, as with the addresses | `tests/edit_checkers_agree.rs` | `a_check_that_passes_carries_its_warnings_in_the_mcp_summary` |
| `ReplaceParam` writes the bytes satz's own writer writes, on an unaligned line and on an aligned one, whose `=` column both keep | `tests/edit_bind_parity.rs` | `on_unaligned_params_replace_param_and_satz_write_the_same_bytes`, `on_an_aligned_line_the_two_keep_the_column_and_write_the_same_bytes` |
| The edit proof in memory: one value's bytes change and nothing else on its line; a param is rewritten in place or appended as `bind` appends it; a node that is not a value, a raw value that breaks the structure, and two edits on one node are refused | `tests/edit_apply.rs` | all |
| `Cst::text()` is the file; every `.satz` under `vendor/satz` parses through the grammar and lowers through `satz_core::satz::parse` | `tests/cst_roundtrip.rs`, `src/cst/grammar.rs` | all |
| `scan_uses` finds every line of a `satz interview --create` skeleton and only the exact shape satz's `pack_line` writes | `tests/cst_uses.rs` | `the_interview_skeleton_is_scanned_line_for_line`, `only_the_exact_shape_is_a_pack_line` |
| The pack rows: the skeleton is the map row then every gated line `Off`; with the map on every row has its question and a `oneof` is its options; a gate without a line is `Absent`; a line active while its gate is false is a `Note` | `tests/model_packs.rs` | all |
| `MIN_SATZ` is the submodule's version; an older binary is refused naming `satz self-update` | `src/satz/binary.rs` | `min_satz_is_the_submodule_version`, `an_older_binary_is_refused_by_version` |
| `McpSession` lists the twenty-two tools of `docs/mcp.md`, reads `runs_as`, types a tool call; `satz_transpile` under `--allow read` is `is_error` | `tests/satz_mcp.rs`, `tests/satz_session.rs` | all |
| The recorded `satz questions --format json` of the pinned satz round-trips through the report types | `src/satz/reports.rs` | `the_recorded_questions_report_round_trips` |
| A reporting call writes one file, reads it back as a typed report and leaves neither the file nor its directory behind, the estate untouched | `tests/satz_cli.rs` | `a_reporting_call_leaves_nothing_behind` |
| The recorded `structuredContent` of a refused `satz_transpile_check` reads as a `Refusal` and as a `CompileSummary`; a summary without `findings` reads, and a kind satz adds is carried | `src/satz/reports.rs` | `a_refusals_structured_content_reads_as_findings`, `a_summary_without_findings_reads_and_a_kind_satz_adds_is_carried` |
| A finding becomes a diagnostic: severity mapped, `kind` carried, a relative file resolved against the estate's directory, the group's header in front of the message with one colon | `src/diag.rs` | `a_finding_keeps_its_kind_and_resolves_its_file_against_the_estate`, `a_group_heads_the_message_and_keeps_one_colon`, `a_note_without_a_group_keeps_its_message_verbatim` |
| The CLI's `CompileRefusal { message, findings }` in its `Debug` form decodes to the same findings, `Some`/`None` fields and escaped quotes included; a shape the parser does not know is one diagnostic with the raw payload; a refused tool result reads its findings from `structuredContent`, falls to its text without them, and is an error on a payload of another shape | `src/edit/check.rs` | `a_compile_refusal_rendered_with_debug_becomes_one_diagnostic_per_finding`, `a_relative_file_in_a_finding_resolves_against_the_estates_directory`, `a_refusal_shape_the_parser_does_not_know_is_one_diagnostic_with_the_payload`, `the_mcp_refusal_reads_its_structured_findings_and_falls_to_the_text_without_them` |
| The agent loop over a scripted provider and a mock host: tool call → result → end of turn, a denied tool is an `is_error` result, a cancelled turn is rolled back whole | `tests/llm_agent.rs` | all |
| The SSE fixtures assemble to the expected responses; the request body carries the breakpoints and headers per credential; the error table | `tests/llm_sse.rs`, `tests/llm_request.rs`, `tests/llm_errors.rs` | all |

## 2. The manual smoke walk

The estate views are checked by hand: [`ui.md`](ui.md), section "The smoke walk" —
seven steps over the fixture estate and a skeleton, no credential needed. It is the
check of the window over what the harness proves of the core.

## 3. Checks that need a credential or a machine

- **Claude, live.** `SATZ_STUDIO_LIVE=1 cargo test -p satz-studio-core --test llm_live`
  sends one prompt with the real tool list twice and asserts
  `cache_read_input_tokens > 0` on the second call. It needs a Claude credential
  (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, an `ant auth login` profile, or the
  keychain entry Settings writes) and runs only with the variable set. In the Chat view,
  "which questions are open?" makes the agent call `satz_questions`, and a write tool
  call shows the approval card first.
- **The bundles.** `.github/workflows/release.yml` builds the `.app` and `.dmg` on
  `macos-15` (arm64) and `macos-15-intel` (x86_64), the `.deb` and `.AppImage` on
  `ubuntu-24.04` and the `.msi` on `windows-2022`. That each bundle launches and opens
  the fixture estate is checked by hand on a machine of that OS: unzip or mount, start
  the app past the unsigned-build prompt the README names, Estates → the repository's
  `tests/fixtures` → Open → the top bar shows `smoke.satz`, "runs as the ADC identity"
  and the schema chip. On Linux the `.deb` declares the runtime libraries; the AppImage
  needs `webkit2gtk-4.1` on the host. On Windows the app needs the `satz` binary, which
  satz does not release for Windows.

## 4. How to run everything

```sh
cargo fmt -p satz-studio-core -p satz-studio -- --check      # the two packages
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked                                # every unit and integration test
bash scripts/e2e.sh                                            # the satz gate, the e2e tests, a build of the app
bash scripts/check-names.sh                                    # the privacy gate over the tree
dx bundle --package satz-studio --platform desktop --release   # this machine's bundle, under target/dx/satz-studio/bundle/
```

`scripts/e2e.sh` checks that `satz --version` is at least `MIN_SATZ`
(`crates/satz-studio-core/src/satz/binary.rs`), runs
`cargo test -p satz-studio-core --locked --test 'e2e_*'`, and without `--ci` also
`cargo build -p satz-studio --locked`; one verdict line per step, and the first failure
ends the run. `ci.yml` runs it with `--ci` after the tests on every push and pull
request. The e2e tests each write a temporary estate whose `config.toml` points into
`vendor/satz` by absolute path, so nothing under the repository is written.
