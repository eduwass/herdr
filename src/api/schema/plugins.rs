use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::common::AgentStatus;
use super::common::SplitDirection;
use super::panes::PaneInfo;
use super::workspaces::WorkspaceWorktreeInfo;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginLinkParams {
    pub path: String,
    #[serde(default = "super::common::default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PluginSourceInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginUnlinkParams {
    pub plugin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSetEnabledParams {
    pub plugin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginInfo {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub min_herdr_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub manifest_path: String,
    pub plugin_root: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<PluginManifestBuild>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PluginManifestAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<PluginManifestEventHook>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<PluginManifestPane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_handlers: Vec<PluginManifestLinkHandler>,
    #[serde(default)]
    pub source: PluginSourceInfo,
    /// Warnings collected at link time or on registry load (e.g. unknown event names,
    /// missing manifest file). Non-fatal — the entry is kept and surfaced by plugin.list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSourceInfo {
    #[serde(default)]
    pub kind: PluginSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_unix_ms: Option<u64>,
}

impl Default for PluginSourceInfo {
    fn default() -> Self {
        Self {
            kind: PluginSourceKind::Local,
            owner: None,
            repo: None,
            subdir: None,
            requested_ref: None,
            resolved_commit: None,
            managed_path: None,
            installed_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginSourceKind {
    #[default]
    Local,
    Github,
}

pub(crate) fn plugin_managed_path_component(value: &str) -> String {
    let slug = readable_plugin_path_slug(value);
    let hash = short_plugin_id_hash_for_path_component(value);
    format!("{slug}-{hash}")
}

fn readable_plugin_path_slug(value: &str) -> String {
    let mut slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug
        .trim_matches(|ch| matches!(ch, '-' | '_' | '.'))
        .to_string();
    let slug = if slug.is_empty() {
        "plugin".to_string()
    } else {
        slug.chars().take(80).collect()
    };
    if has_windows_reserved_stem_for_path_component(&slug) {
        slug.replace('.', "-")
    } else {
        slug
    }
}

pub(crate) fn short_plugin_id_hash_for_path_component(value: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(value.as_bytes());
    let mut hash = String::with_capacity(12);
    for byte in &digest[..6] {
        use std::fmt::Write as _;
        let _ = write!(hash, "{byte:02x}");
    }
    hash
}

pub(crate) fn has_windows_reserved_stem_for_path_component(value: &str) -> bool {
    let stem = value.split('.').next().unwrap_or(value);
    matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_managed_path_component_is_windows_safe_and_collision_free() {
        let dotdot = plugin_managed_path_component("..");
        assert_ne!(dotdot, ".");
        assert_ne!(dotdot, "..");
        assert!(dotdot.starts_with("plugin-"));
        assert_ne!(
            plugin_managed_path_component("a:b"),
            plugin_managed_path_component("a_b")
        );
        assert_ne!(plugin_managed_path_component("con"), "con");
        assert!(!plugin_managed_path_component("example.").ends_with('.'));
        assert!(plugin_managed_path_component("con.example").starts_with("con-example-"));
        assert!(plugin_managed_path_component("aux.plugin").starts_with("aux-plugin-"));
        assert!(plugin_managed_path_component("nul.x").starts_with("nul-x-"));
        assert!(plugin_managed_path_component("com1.tool").starts_with("com1-tool-"));
    }

    #[test]
    fn plugin_managed_path_component_keeps_readable_slug() {
        let component = plugin_managed_path_component("example.worktree-bootstrap");
        assert!(component.starts_with("example.worktree-bootstrap-"));
        assert!(component.len() <= "example.worktree-bootstrap-".len() + 12);
    }

    #[test]
    fn plugin_managed_path_component_hash_distinguishes_same_slug_shape() {
        assert_ne!(
            plugin_managed_path_component("example:a"),
            plugin_managed_path_component("example/a")
        );
    }

    #[test]
    fn popup_dimension_parses_percent_absolute_and_serializes_to_string() {
        let pct: PopupDimension = serde_json::from_str("\"80%\"").unwrap();
        assert_eq!(pct, PopupDimension::Percent(80));
        let cells_str: PopupDimension = serde_json::from_str("\"60\"").unwrap();
        assert_eq!(cells_str, PopupDimension::Cells(60));
        let cells_int: PopupDimension = serde_json::from_str("60").unwrap();
        assert_eq!(cells_int, PopupDimension::Cells(60));

        // Percent clamps to 100, resolves against the axis.
        let over: PopupDimension = serde_json::from_str("\"150%\"").unwrap();
        assert_eq!(over, PopupDimension::Percent(100));

        // Always serializes to a string for stable round-tripping.
        assert_eq!(
            serde_json::to_string(&PopupDimension::Percent(80)).unwrap(),
            "\"80%\""
        );
        assert_eq!(
            serde_json::to_string(&PopupDimension::Cells(60)).unwrap(),
            "\"60\""
        );
    }

    #[test]
    fn popup_spec_omitted_fields_default_to_none() {
        let spec: PopupSpec = serde_json::from_str("{}").unwrap();
        assert_eq!(spec, PopupSpec::default());
        assert!(spec.width.is_none());
        assert!(spec.border.is_none());
        // Default skips all None fields on serialize.
        assert_eq!(serde_json::to_string(&spec).unwrap(), "{}");
    }

    #[test]
    fn popup_spec_border_padding_bg_round_trip() {
        let spec = PopupSpec {
            width: Some(PopupDimension::Percent(70)),
            height: Some(PopupDimension::Cells(20)),
            position: Some(PopupPosition::RelativeCenter),
            border: Some(true),
            border_style: Some(PopupBorderStyle::Double),
            border_color: Some("#ff0000".to_string()),
            padding: Some(2),
            bg: Some("black".to_string()),
            title: Some("Board".to_string()),
            breakpoints: Vec::new(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: PopupSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }

    #[test]
    fn popup_spec_merge_per_call_overrides_manifest_field_by_field() {
        let manifest = PopupSpec {
            width: Some(PopupDimension::Percent(50)),
            height: Some(PopupDimension::Percent(50)),
            position: Some(PopupPosition::RelativeCenter),
            border: Some(true),
            border_style: Some(PopupBorderStyle::Single),
            border_color: Some("blue".to_string()),
            padding: Some(1),
            bg: Some("black".to_string()),
            title: Some("Manifest".to_string()),
            breakpoints: Vec::new(),
        };
        // Per-call sets only some fields; the rest fall back to the manifest.
        let per_call = PopupSpec {
            width: Some(PopupDimension::Cells(90)),
            border: Some(false),
            title: Some("Call".to_string()),
            ..Default::default()
        };
        let merged = manifest.merge(&per_call);
        // Overridden by per-call.
        assert_eq!(merged.width, Some(PopupDimension::Cells(90)));
        assert_eq!(merged.border, Some(false));
        assert_eq!(merged.title.as_deref(), Some("Call"));
        // Inherited from manifest.
        assert_eq!(merged.height, Some(PopupDimension::Percent(50)));
        assert_eq!(merged.position, Some(PopupPosition::RelativeCenter));
        assert_eq!(merged.border_style, Some(PopupBorderStyle::Single));
        assert_eq!(merged.border_color.as_deref(), Some("blue"));
        assert_eq!(merged.padding, Some(1));
        assert_eq!(merged.bg.as_deref(), Some("black"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestBuild {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestAction {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<PluginActionContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestEventHook {
    pub on: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestPane {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
    #[serde(default)]
    pub placement: PluginPanePlacement,
    /// Styling/sizing for `placement = popup`. Manifest-level default; a per-call
    /// `PluginPaneOpenParams.popup` overrides field-by-field via [`PopupSpec::merge`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup: Option<PopupSpec>,
    pub command: Vec<String>,
}

/// A popup dimension: either a percentage of the relevant terminal axis
/// (`"80%"`) or an absolute cell count (`60`). Deserializes from a JSON string
/// or an integer; always serializes to a string for stable round-tripping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupDimension {
    /// Percentage of the terminal axis, clamped to 1..=100.
    Percent(u16),
    /// Absolute cell count.
    Cells(u16),
}

impl Serialize for PopupDimension {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = match self {
            PopupDimension::Percent(pct) => format!("{pct}%"),
            PopupDimension::Cells(cells) => cells.to_string(),
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for PopupDimension {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Int(i64),
            Str(String),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Int(n) => {
                if n < 0 {
                    return Err(serde::de::Error::custom(
                        "popup dimension cannot be negative",
                    ));
                }
                Ok(PopupDimension::Cells(n.min(u16::MAX as i64) as u16))
            }
            Raw::Str(s) => {
                let trimmed = s.trim();
                if let Some(pct) = trimmed.strip_suffix('%') {
                    let pct: u16 = pct
                        .trim()
                        .parse()
                        .map_err(|_| serde::de::Error::custom("invalid popup percent dimension"))?;
                    Ok(PopupDimension::Percent(pct.min(100)))
                } else {
                    let cells: u16 = trimmed
                        .parse()
                        .map_err(|_| serde::de::Error::custom("invalid popup cell dimension"))?;
                    Ok(PopupDimension::Cells(cells))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PopupBorderStyle {
    #[default]
    Single,
    Double,
    Rounded,
    Thick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PopupPosition {
    /// Center against the entire terminal frame, ignoring sidebar/tab layout.
    #[default]
    TotalCenter,
    /// Center against the pane/content area behind the popup.
    RelativeCenter,
}

/// Styling and sizing for a popup pane. All fields optional and additive so
/// callers (manifest or per-call) only specify what they want to override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PopupSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<PopupDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<PopupDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<PopupPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_style: Option<PopupBorderStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<PopupBreakpointSpec>,
}

impl PopupSpec {
    /// Field-by-field merge where `override_` (a per-call spec) wins over `self`
    /// (the manifest default) for every field it sets.
    pub fn merge(&self, override_: &PopupSpec) -> PopupSpec {
        PopupSpec {
            width: override_.width.or(self.width),
            height: override_.height.or(self.height),
            position: override_.position.or(self.position),
            border: override_.border.or(self.border),
            border_style: override_.border_style.or(self.border_style),
            border_color: override_
                .border_color
                .clone()
                .or_else(|| self.border_color.clone()),
            padding: override_.padding.or(self.padding),
            bg: override_.bg.clone().or_else(|| self.bg.clone()),
            title: override_.title.clone().or_else(|| self.title.clone()),
            breakpoints: if override_.breakpoints.is_empty() {
                self.breakpoints.clone()
            } else {
                override_.breakpoints.clone()
            },
        }
    }
}

/// Area-aware popup overrides. `max_cols` / `max_rows` refer to the host tab
/// area in terminal cells. A breakpoint matches when every provided limit
/// matches; later matching breakpoints override earlier ones field-by-field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PopupBreakpointSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<PopupDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<PopupDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<PopupPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_style: Option<PopupBorderStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifestLinkHandler {
    pub id: String,
    pub title: String,
    pub pattern: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginActionListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginLogListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginActionInvokeParams {
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<PluginInvocationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommandLogInfo {
    pub log_id: String,
    pub plugin_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    pub command: Vec<String>,
    pub status: PluginCommandStatus,
    pub started_unix_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_unix_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPlatform {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginActionContext {
    Global,
    Workspace,
    Tab,
    Pane,
    Selection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInvocationContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorkspaceWorktreeInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focused_pane_status: Option<AgentStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clicked_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_handler_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginActionInfo {
    pub plugin_id: String,
    pub action_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contexts: Vec<PluginActionContext>,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platforms: Option<Vec<PluginPlatform>>,
}

impl PluginActionInfo {
    pub fn qualified_id(&self) -> String {
        format!("{}.{}", self.plugin_id, self.action_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPaneOpenParams {
    pub plugin_id: String,
    pub entrypoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PluginPanePlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SplitDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub focus: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// Per-call popup styling/sizing override for `placement = popup`. Merged
    /// over the manifest pane's `popup` default via [`PopupSpec::merge`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub popup: Option<PopupSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginPanePlacement {
    #[default]
    Overlay,
    Split,
    Tab,
    Zoomed,
    /// Ephemeral floating centered terminal rendered on top of the tiled layout,
    /// outside the layout tree. Dropped on session restore and live-handoff.
    Popup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPaneFocusParams {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPaneCloseParams {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPaneInfo {
    pub plugin_id: String,
    pub entrypoint: String,
    pub pane: PaneInfo,
}
