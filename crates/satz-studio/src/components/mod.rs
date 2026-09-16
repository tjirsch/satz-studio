//! The component library: Material 3 Expressive anatomies as Dioxus components, one
//! per file, each a CSS class in `assets/css/components.css`. `docs/ui.md` names the
//! spec page every component follows and what deviates from it.

mod badge;
mod button;
mod card;
mod checkbox;
mod chip;
mod chip_list;
mod dialog;
mod fab;
mod icon;
mod icon_button;
mod list;
mod nav_rail;
mod progress;
mod radio;
mod segmented;
mod snackbar;
mod source_chips;
mod switch;
mod tabs;
mod text_field;
mod tooltip;
mod top_app_bar;
mod tree;
mod typed_field;

pub use badge::Badge;
pub use button::{Button, ButtonGroup, ButtonVariant};
pub use card::{Card, CardVariant};
pub use checkbox::Checkbox;
pub use chip::{Chip, ChipKind};
pub use chip_list::ChipList;
pub use dialog::Dialog;
pub use fab::{Fab, FabSize};
pub use icon::Icon;
pub use icon_button::{IconButton, IconButtonVariant};
pub use list::{List, ListItem};
pub use nav_rail::{NavRail, NavRailItem};
pub use progress::{CircularProgress, LinearProgress};
pub use radio::Radio;
pub use segmented::{Segment, SegmentedButton};
pub use snackbar::Snackbar;
pub use source_chips::SourceChips;
pub use switch::Switch;
pub use tabs::{Tab, Tabs};
pub use text_field::TextField;
pub use tooltip::Tooltip;
pub use top_app_bar::TopAppBar;
pub use tree::{Tree, TreeItem};
pub use typed_field::{Draft, FieldKind, TypedField};
