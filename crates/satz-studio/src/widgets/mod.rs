//! The app's own parts: what the component library has no component for, built from
//! its UI components and arranged in `assets/app.css`. `docs/ui.md` lists them.

mod connector_tree;
mod field;
mod help;
mod source_tags;
mod switch_row;
mod tag_list;
mod tip;
mod tree;
/// The one module named rather than only re-exported: [`typed_field::commits`] is the
/// rule a view chooses its commit behavior by, and the Decisions view's test reads it.
pub(crate) mod typed_field;

pub use connector_tree::{ConnectorBranch, ConnectorLine, ConnectorTree};
pub use field::Field;
pub use help::HelpButton;
pub use source_tags::{SourceTags, value_in_words};
pub use switch_row::SwitchRow;
pub use tag_list::TagList;
pub use tip::Tip;
pub use tree::{Tree, TreeItem};
pub use typed_field::{Draft, FieldKind, TypedField};
