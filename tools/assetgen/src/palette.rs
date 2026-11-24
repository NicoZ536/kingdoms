//! Shared medieval colour palette, so all generated art reads as one consistent style.
//!
//! Values are `0xRRGGBB`; convert with [`crate::draw::rgb`].

/// Dark brown-black used for sprite outlines.
pub const OUTLINE: u32 = 0x24180f;

// Wood.
pub const TIMBER: u32 = 0x5e3f25;
pub const TIMBER_DARK: u32 = 0x3f2a18;
pub const PLANK: u32 = 0x8f6a43;
pub const PLANK_LIGHT: u32 = 0xb08657;
pub const LOG: u32 = 0x7a5534;
pub const BARK: u32 = 0x5b4030;
pub const END_GRAIN: u32 = 0xc9a172;

// Roofs.
pub const THATCH: u32 = 0xc7a157;
pub const THATCH_OLD: u32 = 0xa9884a;
pub const SHINGLE: u32 = 0x7a5638;
pub const TILE_RED: u32 = 0xa9493a;
pub const TILE_DARK: u32 = 0x8a3b30;
pub const SLATE: u32 = 0x58616e;
pub const SLATE_BLUE: u32 = 0x4b5a72;

// Walls.
pub const PLASTER: u32 = 0xe4d9bf;
pub const DAUB: u32 = 0xcbb58e;
pub const WHITEWASH: u32 = 0xeeeadf;
pub const STONE: u32 = 0x9c968b;
pub const STONE_DARK: u32 = 0x76716a;
pub const STONE_WARM: u32 = 0xb3a78e;
pub const BRICK: u32 = 0xa4553e;

// Ground.
pub const EARTH: u32 = 0x8d7350;
pub const EARTH_DARK: u32 = 0x6e5638;
pub const COBBLE: u32 = 0x8e8a82;
pub const GRASS: u32 = 0x6e9a3f;
pub const GRASS_DARK: u32 = 0x557c30;
pub const SAND: u32 = 0xdcc68e;
pub const WATER: u32 = 0x3b86b0;

// Accents.
pub const IRON: u32 = 0x5d6066;
pub const STEEL: u32 = 0xb9bec6;
pub const GOLD: u32 = 0xd9a930;
pub const GOLD_LIGHT: u32 = 0xf4d469;
pub const GLOW: u32 = 0xf08a2a;
pub const WINDOW: u32 = 0x26303c;
pub const DOOR: u32 = 0x5a3a22;
pub const BANNER_BLUE: u32 = 0x2f4f8f;
pub const BANNER_RED: u32 = 0x9b2d2d;
pub const STRAW: u32 = 0xd8bd6a;

// Kingdoms: field colours of the crests by colour index (the game's `KINGDOM_COLORS`).
pub const KINGDOM_COLORS: [u32; 7] = [0xb02630, 0x2c56ac, 0x307c40, 0x763a92, 0xd47c22, 0x18848e, 0xc44884];

// UI.
pub const PARCHMENT: u32 = 0xead9b0;
pub const PARCHMENT_DARK: u32 = 0xc9b07a;
pub const INK: u32 = 0x3a2a1a;
pub const WOOD_UI: u32 = 0x5a3b22;

// Skin and hair tones for characters.
pub const SKIN: u32 = 0xe0b48a;
pub const HAIR_BROWN: u32 = 0x5a3a20;
pub const HAIR_BLOND: u32 = 0xc9a45a;
pub const HAIR_BLACK: u32 = 0x2b2420;
