//! Raw FFI bindings to the Live2D Cubism SDK Core (v5).
//!
//! These bindings correspond to `Live2DCubismCore.h` (SDK v5).
//! All functions are unsafe — see the Cubism SDK documentation for usage.
//!
//! # Memory Alignment
//!
//! - Moc data must be aligned to [`ALIGN_OF_MOC`] (64 bytes).
//! - Model memory must be aligned to [`ALIGN_OF_MODEL`] (16 bytes).

#![allow(non_camel_case_types, non_upper_case_globals)]

use std::os::raw::{c_char, c_float, c_int, c_uchar, c_uint, c_ushort, c_void};

// ── Opaque types ────────────────────────────────────────────────────────

/// Opaque Cubism moc handle.
#[repr(C)]
pub struct csmMoc {
    _opaque: [u8; 0],
}

/// Opaque Cubism model handle.
#[repr(C)]
pub struct csmModel {
    _opaque: [u8; 0],
}

// ── Scalar types ────────────────────────────────────────────────────────

/// Cubism version identifier.
pub type csmVersion = c_uint;

/// Moc file format version.
pub type csmMocVersion = c_uint;

/// Bitfield flags.
pub type csmFlags = c_uchar;

/// Parameter type.
pub type csmParameterType = c_int;

// ── Vector types ────────────────────────────────────────────────────────

/// 2-component vector (X, Y).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct csmVector2 {
    pub x: c_float,
    pub y: c_float,
}

/// 4-component vector (X, Y, Z, W).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct csmVector4 {
    pub x: c_float,
    pub y: c_float,
    pub z: c_float,
    pub w: c_float,
}

// ── Constants ───────────────────────────────────────────────────────────

/// Required alignment for moc memory (bytes).
pub const ALIGN_OF_MOC: usize = 64;

/// Required alignment for model memory (bytes).
pub const ALIGN_OF_MODEL: usize = 16;

// Constant drawable flags (non-dynamic)
pub const BLEND_ADDITIVE: csmFlags = 1 << 0;
pub const BLEND_MULTIPLICATIVE: csmFlags = 1 << 1;
pub const IS_DOUBLE_SIDED: csmFlags = 1 << 2;
pub const IS_INVERTED_MASK: csmFlags = 1 << 3;

// Dynamic drawable flags
pub const IS_VISIBLE: csmFlags = 1 << 0;
pub const VISIBILITY_DID_CHANGE: csmFlags = 1 << 1;
pub const OPACITY_DID_CHANGE: csmFlags = 1 << 2;
pub const DRAW_ORDER_DID_CHANGE: csmFlags = 1 << 3;
pub const RENDER_ORDER_DID_CHANGE: csmFlags = 1 << 4;
pub const VERTEX_POSITIONS_DID_CHANGE: csmFlags = 1 << 5;
pub const BLEND_COLOR_DID_CHANGE: csmFlags = 1 << 6;

// Moc versions
pub const MOC_VERSION_UNKNOWN: csmMocVersion = 0;
pub const MOC_VERSION_30: csmMocVersion = 1;
pub const MOC_VERSION_33: csmMocVersion = 2;
pub const MOC_VERSION_40: csmMocVersion = 3;
pub const MOC_VERSION_42: csmMocVersion = 4;
pub const MOC_VERSION_50: csmMocVersion = 5;
pub const MOC_VERSION_53: csmMocVersion = 6;

// Parameter types
pub const PARAMETER_TYPE_NORMAL: csmParameterType = 0;
pub const PARAMETER_TYPE_BLEND_SHAPE: csmParameterType = 1;

/// Log handler function pointer.
pub type csmLogFunction = Option<extern "C" fn(message: *const c_char)>;

// ── FFI declarations ────────────────────────────────────────────────────

extern "C" {
    // ── Version ──

    pub fn csmGetVersion() -> csmVersion;
    pub fn csmGetLatestMocVersion() -> csmMocVersion;
    pub fn csmGetMocVersion(address: *const c_void, size: c_uint) -> csmMocVersion;

    // ── Consistency ──

    pub fn csmHasMocConsistency(address: *mut c_void, size: c_uint) -> c_int;

    // ── Logging ──

    pub fn csmGetLogFunction() -> csmLogFunction;
    pub fn csmSetLogFunction(handler: csmLogFunction);

    // ── Moc ──

    pub fn csmReviveMocInPlace(address: *mut c_void, size: c_uint) -> *mut csmMoc;

    // ── Model ──

    pub fn csmGetSizeofModel(moc: *const csmMoc) -> c_uint;
    pub fn csmInitializeModelInPlace(
        moc: *const csmMoc,
        address: *mut c_void,
        size: c_uint,
    ) -> *mut csmModel;
    pub fn csmUpdateModel(model: *mut csmModel);

    /// SDK v5: model-level render orders (replaces csmGetDrawableRenderOrders).
    pub fn csmGetRenderOrders(model: *const csmModel) -> *const c_int;

    // ── Canvas ──

    pub fn csmReadCanvasInfo(
        model: *const csmModel,
        out_size_in_pixels: *mut csmVector2,
        out_origin_in_pixels: *mut csmVector2,
        out_pixels_per_unit: *mut c_float,
    );

    // ── Parameters ──

    pub fn csmGetParameterCount(model: *const csmModel) -> c_int;
    pub fn csmGetParameterIds(model: *const csmModel) -> *const *const c_char;
    pub fn csmGetParameterTypes(model: *const csmModel) -> *const csmParameterType;
    pub fn csmGetParameterMinimumValues(model: *const csmModel) -> *const c_float;
    pub fn csmGetParameterMaximumValues(model: *const csmModel) -> *const c_float;
    pub fn csmGetParameterDefaultValues(model: *const csmModel) -> *const c_float;
    pub fn csmGetParameterValues(model: *mut csmModel) -> *mut c_float;
    pub fn csmGetParameterRepeats(model: *const csmModel) -> *const c_int;
    pub fn csmGetParameterKeyCounts(model: *const csmModel) -> *const c_int;
    pub fn csmGetParameterKeyValues(model: *const csmModel) -> *const *const c_float;

    // ── Parts ──

    pub fn csmGetPartCount(model: *const csmModel) -> c_int;
    pub fn csmGetPartIds(model: *const csmModel) -> *const *const c_char;
    pub fn csmGetPartOpacities(model: *mut csmModel) -> *mut c_float;
    pub fn csmGetPartParentPartIndices(model: *const csmModel) -> *const c_int;
    pub fn csmGetPartOffscreenIndices(model: *const csmModel) -> *const c_int;

    // ── Drawables ──

    pub fn csmGetDrawableCount(model: *const csmModel) -> c_int;
    pub fn csmGetDrawableIds(model: *const csmModel) -> *const *const c_char;
    pub fn csmGetDrawableConstantFlags(model: *const csmModel) -> *const csmFlags;
    pub fn csmGetDrawableDynamicFlags(model: *const csmModel) -> *const csmFlags;
    /// SDK v5: explicit blend mode per drawable.
    pub fn csmGetDrawableBlendModes(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableTextureIndices(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableDrawOrders(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableOpacities(model: *const csmModel) -> *const c_float;
    pub fn csmGetDrawableMaskCounts(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableMasks(model: *const csmModel) -> *const *const c_int;
    pub fn csmGetDrawableVertexCounts(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableVertexPositions(model: *const csmModel) -> *const *const csmVector2;
    pub fn csmGetDrawableVertexUvs(model: *const csmModel) -> *const *const csmVector2;
    pub fn csmGetDrawableIndexCounts(model: *const csmModel) -> *const c_int;
    pub fn csmGetDrawableIndices(model: *const csmModel) -> *const *const c_ushort;
    pub fn csmGetDrawableMultiplyColors(model: *const csmModel) -> *const csmVector4;
    pub fn csmGetDrawableScreenColors(model: *const csmModel) -> *const csmVector4;
    pub fn csmGetDrawableParentPartIndices(model: *const csmModel) -> *const c_int;
    pub fn csmResetDrawableDynamicFlags(model: *mut csmModel);

    // ── Offscreens (SDK v5) ──

    pub fn csmGetOffscreenCount(model: *const csmModel) -> c_int;
    pub fn csmGetOffscreenBlendModes(model: *const csmModel) -> *const c_int;
    pub fn csmGetOffscreenOpacities(model: *const csmModel) -> *const c_float;
    pub fn csmGetOffscreenOwnerIndices(model: *const csmModel) -> *const c_int;
    pub fn csmGetOffscreenMultiplyColors(model: *const csmModel) -> *const csmVector4;
    pub fn csmGetOffscreenScreenColors(model: *const csmModel) -> *const csmVector4;
    pub fn csmGetOffscreenMaskCounts(model: *const csmModel) -> *const c_int;
    pub fn csmGetOffscreenMasks(model: *const csmModel) -> *const *const c_int;
    pub fn csmGetOffscreenConstantFlags(model: *const csmModel) -> *const csmFlags;
}
