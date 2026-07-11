//! Shared pass access summaries for render graph analysis and diagnostics.

use std::borrow::Cow;

use super::types::*;

/// Borrowed summary of a pass's declared resource access contract.
///
/// This is generated from [`PassEntry`] on demand so it cannot drift from the
/// graph's declaration source of truth and does not clone large copy payloads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PassAccessInfo<'a> {
    pub pass_index: usize,
    pub name: &'a Cow<'static, str>,
    pub pass_type: PassType,
    pub flags: PassFlags,
    pub reads: &'a [ResourceRef],
    pub writes: &'a [ResourceRef],
    pub color_outputs: &'a [ColorOutput],
    pub depth_stencil: Option<DepthStencilOutput>,
    pub copy_ops: &'a [CopyOp],
}

impl PassEntry {
    #[inline]
    pub(crate) fn access_info(&self, pass_index: usize) -> PassAccessInfo<'_> {
        PassAccessInfo {
            pass_index,
            name: &self.name,
            pass_type: self.pass_type,
            flags: self.flags,
            reads: &self.reads,
            writes: &self.writes,
            color_outputs: &self.color_outputs,
            depth_stencil: self.depth_stencil,
            copy_ops: &self.copy_ops,
        }
    }
}
