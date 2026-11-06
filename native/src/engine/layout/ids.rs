//! Interned class ids, property names and enum item names the layout reads, resolved once per
//! engine. Enum comparisons in the Luau reference are by item *name* (`EnumName(Value) == "X"`), so
//! every enum item gets a small name code (`E::*`) from a table indexed by its id: no string
//! compares on the hot path, and the same result as the name comparison for any enum type.

use crate::engine::{
    ClassId, NameId, Reflection,
    reflection::EnumItemId,
    value::Value,
};

/// Enum item name codes (`EnumName` results the layout compares against).
#[allow(non_snake_case)]
pub mod E {
    /// `EnumName(nil)` (the property is nil / missing)
    pub const EMPTY: u8 = 0;
    /// Any name the layout does not compare against
    pub const OTHER: u8 = 1;
    pub const NONE: u8 = 2;
    pub const X: u8 = 3;
    pub const Y: u8 = 4;
    pub const XY: u8 = 5;
    pub const HORIZONTAL: u8 = 6;
    pub const VERTICAL: u8 = 7;
    pub const CENTER: u8 = 8;
    pub const BOTTOM: u8 = 9;
    pub const RIGHT: u8 = 10;
    pub const LEFT: u8 = 11;
    pub const FILL: u8 = 12;
    pub const SPACE_BETWEEN: u8 = 13;
    pub const SPACE_AROUND: u8 = 14;
    pub const SPACE_EVENLY: u8 = 15;
    pub const AUTOMATIC: u8 = 16;
    pub const STRETCH: u8 = 17;
    pub const START: u8 = 18;
    pub const END: u8 = 19;
    pub const NAME: u8 = 20;
    pub const LAYOUT_ORDER: u8 = 21;
    pub const GROW: u8 = 22;
    pub const SHRINK: u8 = 23;
    pub const CUSTOM: u8 = 24;
    pub const SCALE_WITH_PARENT_SIZE: u8 = 25;
    pub const HEIGHT: u8 = 26;
    pub const RELATIVE_XX: u8 = 27;
    pub const RELATIVE_YY: u8 = 28;
    pub const OUTLINE: u8 = 29;
    pub const INSET: u8 = 30;
    pub const TOP_RIGHT: u8 = 31;
    pub const BOTTOM_LEFT: u8 = 32;
    pub const BOTTOM_RIGHT: u8 = 33;
    pub const COLUMN_MAJOR: u8 = 34;
    pub const GLOBAL: u8 = 35;
    pub const ITALIC: u8 = 36;
    pub const AT_END: u8 = 37;
    pub const SPLIT_WORD: u8 = 38;
    pub const TOP: u8 = 39;

    pub const NAMES: &[(&str, u8)] = &[
        ("None", NONE),
        ("X", X),
        ("Y", Y),
        ("XY", XY),
        ("Horizontal", HORIZONTAL),
        ("Vertical", VERTICAL),
        ("Center", CENTER),
        ("Bottom", BOTTOM),
        ("Right", RIGHT),
        ("Left", LEFT),
        ("Fill", FILL),
        ("SpaceBetween", SPACE_BETWEEN),
        ("SpaceAround", SPACE_AROUND),
        ("SpaceEvenly", SPACE_EVENLY),
        ("Automatic", AUTOMATIC),
        ("Stretch", STRETCH),
        ("Start", START),
        ("End", END),
        ("Name", NAME),
        ("LayoutOrder", LAYOUT_ORDER),
        ("Grow", GROW),
        ("Shrink", SHRINK),
        ("Custom", CUSTOM),
        ("ScaleWithParentSize", SCALE_WITH_PARENT_SIZE),
        ("Height", HEIGHT),
        ("RelativeXX", RELATIVE_XX),
        ("RelativeYY", RELATIVE_YY),
        ("Outline", OUTLINE),
        ("Inset", INSET),
        ("TopRight", TOP_RIGHT),
        ("BottomLeft", BOTTOM_LEFT),
        ("BottomRight", BOTTOM_RIGHT),
        ("ColumnMajor", COLUMN_MAJOR),
        ("Global", GLOBAL),
        ("Italic", ITALIC),
        ("AtEnd", AT_END),
        ("SplitWord", SPLIT_WORD),
        ("Top", TOP),
    ];
}

/// Property-name classes for the change hook (install.luau's tables).
#[allow(non_snake_case)]
pub mod P {
    pub const COMPUTED: u8 = 1;
    pub const APPEARANCE: u8 = 2;
    pub const PLACEMENT: u8 = 4;
}

const COMPUTED: &[&str] = &[
    "AbsolutePosition",
    "AbsoluteSize",
    "AbsoluteRotation",
    "TextBounds",
    "TextFits",
    "AbsoluteContentSize",
    "AbsoluteCellSize",
    "AbsoluteCellCount",
    "AbsoluteCanvasSize",
    "AbsoluteWindowSize",
    "GuiState",
    "ContentText",
    "LocalizedText",
    "IsLoaded",
    "CurrentPage",
];

const APPEARANCE: &[&str] = &[
    "Active",
    "Archivable",
    "AutoButtonColor",
    "AutoLocalize",
    "BackgroundColor",
    "BackgroundColor3",
    "BackgroundTransparency",
    "BorderColor",
    "BorderColor3",
    "BottomImage",
    "ClearTextOnFocus",
    "ClipsDescendants",
    "CursorPosition",
    "ElasticBehavior",
    "GroupColor3",
    "GroupTransparency",
    "HoverImage",
    "Image",
    "ImageColor3",
    "ImageContent",
    "ImageRectOffset",
    "ImageRectSize",
    "ImageTransparency",
    "Interactable",
    "MaxVisibleGraphemes",
    "MidImage",
    "Modal",
    "NextSelectionDown",
    "NextSelectionLeft",
    "NextSelectionRight",
    "NextSelectionUp",
    "PlaceholderColor3",
    "PressedImage",
    "ResampleMode",
    "ScaleType",
    "ScrollBarImageColor3",
    "ScrollBarImageTransparency",
    "ScrollingEnabled",
    "Selectable",
    "Selected",
    "SelectionGroup",
    "SelectionImageObject",
    "SelectionOrder",
    "SelectionStart",
    "ShowNativeInput",
    "SliceCenter",
    "SliceScale",
    "Style",
    "TextColor",
    "TextColor3",
    "TextEditable",
    "TextStrokeColor3",
    "TextStrokeTransparency",
    "TextTransparency",
    "TextXAlignment",
    "TextYAlignment",
    "TileSize",
    "TopImage",
    "ZIndex",
];

const PLACEMENT: &[&str] = &["AnchorPoint", "LayoutOrder", "Name", "Position", "Rotation", "Visible"];

/// A name id that no instance ever has (for names the reflection does not know).
const MISSING: NameId = NameId::MAX;

macro_rules! names {
    ($($field:ident),* $(,)?) => {
        /// Property / member names by interned id.
        #[allow(non_snake_case)]
        #[derive(Clone, Copy)]
        pub struct Names { $(pub $field: NameId),* }
        impl Names {
            fn resolve(reflection: &Reflection) -> Names {
                Names { $($field: reflection.name_id(stringify!($field)).unwrap_or(MISSING)),* }
            }
        }
    };
}

names!(
    Size, Position, AnchorPoint, Rotation, Visible, AutomaticSize, SizeConstraint, BorderMode,
    BorderSizePixel, PaddingLeft, PaddingRight, PaddingTop, PaddingBottom, MinSize, MaxSize,
    AspectRatio, AspectType, DominantAxis, Scale, Text, PlaceholderText, RichText, FontFace,
    TextWrapped, TextScaled, TextSize, LineHeight, TextTruncate, TextXAlignment, TextYAlignment,
    MinTextSize, MaxTextSize, LayoutOrder, SortOrder, FillDirection, HorizontalAlignment,
    VerticalAlignment, Padding, Wraps, HorizontalFlex, VerticalFlex, ItemLineAlignment, FlexMode,
    GrowRatio, ShrinkRatio, CellSize, CellPadding, FillDirectionMaxCells, StartCorner, CurrentPage,
    MajorAxis, FillEmptySpaceColumns, FillEmptySpaceRows, ScrollBarThickness, CanvasSize,
    ScrollingDirection, AutomaticCanvasSize, VerticalScrollBarInset, HorizontalScrollBarInset,
    VerticalScrollBarPosition, CanvasPosition, ScreenInsets, Enabled, DisplayOrder,
    AbsolutePosition, AbsoluteSize, AbsoluteRotation, TextBounds, TextFits, AbsoluteContentSize,
    AbsoluteCellSize, AbsoluteCellCount, AbsoluteCanvasSize, AbsoluteWindowSize, Circular,
);

macro_rules! classes {
    ($($field:ident),* $(,)?) => {
        /// Class ids (`u32::MAX` when the reflection lacks the class: never matches).
        #[allow(non_snake_case)]
        #[derive(Clone, Copy)]
        pub struct Classes { $(pub $field: ClassId),* }
        impl Classes {
            fn resolve(reflection: &Reflection) -> Classes {
                Classes { $($field: reflection.class_id(stringify!($field)).unwrap_or(ClassId::MAX)),* }
            }
        }
    };
}

classes!(
    GuiBase2d, GuiObject, LayerCollector, ScreenGui, SurfaceGui, Folder, ScrollingFrame, TextLabel,
    TextButton, TextBox, UIGridStyleLayout, UIListLayout, UIGridLayout, UIPageLayout, UITableLayout,
    UIScale, UIPadding, UISizeConstraint, UIAspectRatioConstraint, UITextSizeConstraint, UIFlexItem,
    UIComponent, UIBase, CanvasGroup, PlayerGui, CoreGui, StarterGui, PluginGui,
    DockWidgetPluginGui, Players, UICorner, UIDragDetector, UIGradient, UIStroke, ImageLabel,
    ImageButton, ViewportFrame, UIShadow, GuiButton,
);

/// Everything the layout resolves once per engine.
pub struct Ids {
    pub n: Names,
    pub c: Classes,
    /// Enum item id -> name code
    enum_codes: Vec<u8>,
    /// Name id -> P::* flags
    name_flags: Vec<u8>,
    pub automatic_none: Option<EnumItemId>,
    /// ScreenInsets.CoreUISafeInsets: the one ScreenGui inset that starts below the topbar
    /// (IgnoreGuiInset is a view of it, derived.rs)
    pub core_ui_safe_insets: Option<EnumItemId>,
}

impl Ids {
    pub fn new(reflection: &Reflection) -> Ids {
        let mut enum_codes = vec![E::OTHER; reflection.enum_items.len()];
        for (index, item) in reflection.enum_items.iter().enumerate() {
            if let Some((_, code)) = E::NAMES.iter().find(|(name, _)| **name == *item.name) {
                enum_codes[index] = *code;
            }
        }
        let mut name_flags = Vec::new();
        for (list, flag) in [(COMPUTED, P::COMPUTED), (APPEARANCE, P::APPEARANCE), (PLACEMENT, P::PLACEMENT)] {
            for name in list {
                if let Some(id) = reflection.name_id(name) {
                    let index = id as usize;
                    if name_flags.len() <= index {
                        name_flags.resize(index + 1, 0);
                    }
                    name_flags[index] |= flag;
                }
            }
        }
        Ids {
            n: Names::resolve(reflection),
            c: Classes::resolve(reflection),
            enum_codes,
            name_flags,
            automatic_none: reflection.enum_item("AutomaticSize", "None"),
            core_ui_safe_insets: reflection.enum_item("ScreenInsets", "CoreUISafeInsets"),
        }
    }

    /// `EnumName(Value)` as a code.
    #[inline]
    pub fn code(&self, value: Option<&Value>) -> u8 {
        match value {
            None => E::EMPTY,
            Some(Value::EnumItem(item)) => self.enum_codes.get(*item as usize).copied().unwrap_or(E::OTHER),
            Some(_) => E::OTHER,
        }
    }

    #[inline]
    pub fn name_flags(&self, name: NameId) -> u8 {
        self.name_flags.get(name as usize).copied().unwrap_or(0)
    }
}
