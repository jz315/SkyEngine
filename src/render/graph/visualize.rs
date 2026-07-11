//! GraphViz DOT export for render graph visualization.

use std::fmt::Write;

use super::*;

/// Options for exporting a render graph as GraphViz DOT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderGraphDotOptions {
    /// Include compile-time details such as execution index, dependency level,
    /// pass flags, resource lifetime, liveness, and external source/sink state.
    pub include_debug_details: bool,
    /// Include alias group nodes and dashed links to their virtual texture
    /// members. Alias groups are only available after physical allocation.
    pub include_alias_groups: bool,
}

impl RenderGraphDotOptions {
    /// Compact DOT output matching the historical `export_dot()` shape.
    pub fn compact() -> Self {
        Self::default()
    }

    /// Detailed DOT output useful for render graph diagnostics.
    pub fn detailed() -> Self {
        Self {
            include_debug_details: true,
            include_alias_groups: true,
        }
    }
}

impl RenderGraph {
    /// Export the render graph as a GraphViz DOT string.
    ///
    /// For accurate alive/culled state, call [`Self::compile`] first.
    pub fn export_dot(&self) -> String {
        self.export_dot_with_options(RenderGraphDotOptions::compact())
    }

    /// Export the render graph as GraphViz DOT with explicit detail options.
    ///
    /// For accurate alive/culled state, call [`compile`](Self::compile) first.
    /// For alias group detail, call
    /// [`allocate_physical_resources`](Self::allocate_physical_resources)
    /// first.
    pub fn export_dot_with_options(&self, options: RenderGraphDotOptions) -> String {
        if !self.compiled {
            eprintln!(
                "[SkyEngine] RenderGraph::export_dot() called before compile(); \
                 alive/culled status may be inaccurate"
            );
        }

        // Escape characters that are special in DOT label strings.
        fn dot_escape(s: &str) -> String {
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('<', "\\<")
                .replace('>', "\\>")
        }
        fn dot_label(lines: &[String]) -> String {
            lines
                .iter()
                .map(|line| dot_escape(line))
                .collect::<Vec<_>>()
                .join("\\n")
        }
        fn resource_node_id(resource: &ResourceRef) -> String {
            match resource {
                ResourceRef::Texture(handle) => format!("res_tex_{}", handle.0),
                ResourceRef::TextureSubresource(subresource) => {
                    format!("res_tex_{}", subresource.texture.0)
                }
                ResourceRef::Buffer(handle) => format!("res_buf_{}", handle.0),
                ResourceRef::Surface => "res_surface".to_string(),
            }
        }
        fn subresource_label(resource: &ResourceRef) -> Option<String> {
            match resource {
                ResourceRef::TextureSubresource(subresource) => Some(format!(
                    "m{}+{} l{}+{}",
                    subresource.base_mip_level,
                    subresource.mip_level_count,
                    subresource.base_array_layer,
                    subresource.array_layer_count
                )),
                _ => None,
            }
        }
        fn resource_debug_lines(resource: &RenderGraphResourceDebug) -> Vec<String> {
            let mut lines = Vec::new();
            let residency = if resource.imported {
                "imported"
            } else if resource.persistent {
                "persistent"
            } else if resource.transient {
                "transient"
            } else {
                "external"
            };
            lines.push(residency.to_string());
            lines.push(if resource.live { "live" } else { "not live" }.to_string());
            if resource.external_source {
                lines.push("external source".to_string());
            }
            if resource.external_sink {
                lines.push("external sink".to_string());
            }
            lines
        }
        fn copy_op_label(op: &CopyOpDebug) -> String {
            match op {
                CopyOpDebug::TextureToTexture { .. } => "TextureToTexture".to_string(),
                CopyOpDebug::BufferToBuffer { .. } => "BufferToBuffer".to_string(),
                CopyOpDebug::BufferToTexture { .. } => "BufferToTexture".to_string(),
                CopyOpDebug::UploadToTexture { data_len, .. } => {
                    format!("UploadToTexture({data_len} bytes)")
                }
            }
        }

        let dump = (options.include_debug_details || options.include_alias_groups)
            .then(|| self.debug_dump());
        let resources_by_ref = dump.as_ref().map(|dump| {
            dump.resources
                .iter()
                .map(|resource| (resource.resource, resource))
                .collect::<FxHashMap<_, _>>()
        });
        let lifetimes_by_ref = dump.as_ref().map(|dump| {
            dump.lifetimes
                .iter()
                .map(|lifetime| (lifetime.resource, lifetime))
                .collect::<FxHashMap<_, _>>()
        });
        let alias_membership = dump.as_ref().map(|dump| {
            let mut membership = FxHashMap::default();
            for group in &dump.alias_groups {
                if group.members.len() <= 1 {
                    continue;
                }
                for member in &group.members {
                    membership.insert(member.texture_index, group.group_index);
                }
            }
            membership
        });

        let mut dot = String::with_capacity(2048);
        writeln!(dot, "digraph RenderGraph {{").unwrap();
        writeln!(dot, "    rankdir=LR;").unwrap();
        writeln!(
            dot,
            "    graph [fontname=\"Helvetica\", bgcolor=\"#1a1a2e\"];"
        )
        .unwrap();
        writeln!(
            dot,
            "    node [fontname=\"Helvetica\", fontcolor=\"white\"];"
        )
        .unwrap();
        writeln!(dot, "    edge [color=\"#aaaacc\"];").unwrap();
        writeln!(dot).unwrap();

        // Resource nodes
        writeln!(dot, "    // Resources").unwrap();
        for (i, tex) in self.textures.iter().enumerate() {
            let resource = ResourceRef::Texture(TextureHandle(i, self.handle_token));
            let mut label_lines = vec![
                tex.name.to_string(),
                format!("{:?} {:?}", tex.size, tex.format),
            ];
            if let Some(resources) = resources_by_ref.as_ref() {
                if let Some(debug) = resources.get(&resource) {
                    label_lines.extend(resource_debug_lines(debug));
                }
            }
            if let Some(lifetimes) = lifetimes_by_ref.as_ref() {
                if let Some(lifetime) = lifetimes.get(&resource) {
                    label_lines.push(format!(
                        "lifetime {}..{}",
                        lifetime.first_use, lifetime.last_use
                    ));
                }
            }
            if let Some(membership) = alias_membership.as_ref() {
                if let Some(group_index) = membership.get(&i) {
                    label_lines.push(format!("alias group {group_index}"));
                }
            }
            writeln!(
                dot,
                "    res_tex_{i} [label=\"{label}\", \
                 shape=box, style=filled, fillcolor=\"#2d2d44\", color=\"#6c6c8a\"];",
                label = dot_label(&label_lines),
            )
            .unwrap();
        }
        for (i, buf) in self.buffers.iter().enumerate() {
            let resource = ResourceRef::Buffer(BufferHandle(i, self.handle_token));
            let mut label_lines = vec![buf.name.to_string(), format!("{} bytes", buf.size_bytes)];
            if let Some(resources) = resources_by_ref.as_ref() {
                if let Some(debug) = resources.get(&resource) {
                    label_lines.extend(resource_debug_lines(debug));
                }
            }
            if let Some(lifetimes) = lifetimes_by_ref.as_ref() {
                if let Some(lifetime) = lifetimes.get(&resource) {
                    label_lines.push(format!(
                        "lifetime {}..{}",
                        lifetime.first_use, lifetime.last_use
                    ));
                }
            }
            writeln!(
                dot,
                "    res_buf_{i} [label=\"{label}\", \
                 shape=box, style=filled, fillcolor=\"#243447\", color=\"#4f81bd\"];",
                label = dot_label(&label_lines),
            )
            .unwrap();
        }
        let mut surface_label = vec!["Surface".to_string()];
        if let Some(resources) = resources_by_ref.as_ref() {
            if let Some(debug) = resources.get(&ResourceRef::Surface) {
                surface_label.extend(resource_debug_lines(debug));
            }
        }
        if let Some(lifetimes) = lifetimes_by_ref.as_ref() {
            if let Some(lifetime) = lifetimes.get(&ResourceRef::Surface) {
                surface_label.push(format!(
                    "lifetime {}..{}",
                    lifetime.first_use, lifetime.last_use
                ));
            }
        }
        writeln!(
            dot,
            "    res_surface [label=\"{label}\", shape=box, style=filled, \
             fillcolor=\"#44223d\", color=\"#8a4477\"];",
            label = dot_label(&surface_label),
        )
        .unwrap();
        writeln!(dot).unwrap();

        // Pass nodes
        writeln!(dot, "    // Passes").unwrap();
        for (i, pass) in self.passes.iter().enumerate() {
            let debug_pass = dump.as_ref().and_then(|dump| {
                dump.passes
                    .iter()
                    .find(|debug| debug.declaration_order == i)
            });
            let mut label_lines = vec![pass.name.to_string(), format!("({:?})", pass.pass_type)];
            if let Some(debug_pass) = debug_pass {
                label_lines.push(format!("decl #{}", debug_pass.declaration_order));
                match debug_pass.execution_order {
                    Some(order) => label_lines.push(format!("exec #{order}")),
                    None if self.compiled => label_lines.push("culled".to_string()),
                    None => label_lines.push("uncompiled".to_string()),
                }
                label_lines.push(format!("dep {}", debug_pass.dep_level));
                if !debug_pass.flags.is_empty() {
                    label_lines.push(format!("flags {:?}", debug_pass.flags));
                }
                if !debug_pass.copy_ops.is_empty() {
                    let ops = debug_pass
                        .copy_ops
                        .iter()
                        .map(copy_op_label)
                        .collect::<Vec<_>>()
                        .join(", ");
                    label_lines.push(format!("copy: {ops}"));
                }
                if !debug_pass.color_outputs.is_empty() {
                    let outputs = debug_pass
                        .color_outputs
                        .iter()
                        .map(|output| format!("slot {}", output.slot))
                        .collect::<Vec<_>>()
                        .join(", ");
                    label_lines.push(format!("colors: {outputs}"));
                }
                if debug_pass.depth_stencil.is_some() {
                    label_lines.push("depth/stencil".to_string());
                }
            }
            let (fill, border) = if !pass.alive {
                ("\"#3a3a3a\"", "\"#666666\"")
            } else {
                match pass.pass_type {
                    PassType::Render => ("\"#1b4f72\"", "\"#5dade2\""),
                    PassType::Compute => ("\"#1e6a4b\"", "\"#52be80\""),
                    PassType::Copy => ("\"#7d6608\"", "\"#f4d03f\""),
                }
            };
            let style = if pass.alive {
                "filled"
            } else {
                "filled,dashed"
            };
            writeln!(
                dot,
                "    pass_{i} [label=\"{label}\", \
                 shape=ellipse, style=\"{style}\", fillcolor={fill}, color={border}];",
                label = dot_label(&label_lines),
            )
            .unwrap();
        }
        writeln!(dot).unwrap();

        if options.include_alias_groups {
            if let Some(dump) = dump.as_ref() {
                writeln!(dot, "    // Alias groups").unwrap();
                for group in dump
                    .alias_groups
                    .iter()
                    .filter(|group| group.members.len() > 1)
                {
                    writeln!(
                        dot,
                        "    alias_group_{idx} [label=\"Alias group {idx}\\n{format:?} \
                         {width}x{height}\", shape=diamond, style=filled, \
                         fillcolor=\"#3b2f4a\", color=\"#aa88dd\"];",
                        idx = group.group_index,
                        format = group.format,
                        width = group.width,
                        height = group.height,
                    )
                    .unwrap();
                    for member in &group.members {
                        writeln!(
                            dot,
                            "    alias_group_{idx} -> res_tex_{tex} [style=dashed, \
                             color=\"#aa88dd\", label=\"{role}\"];",
                            idx = group.group_index,
                            tex = member.texture_index,
                            role = if member.primary { "primary" } else { "alias" },
                        )
                        .unwrap();
                    }
                }
                writeln!(dot).unwrap();
            }
        }

        // Edges
        writeln!(dot, "    // Edges").unwrap();
        for (i, pass) in self.passes.iter().enumerate() {
            for r in &pass.reads {
                let res_id = resource_node_id(r);
                if let Some(label) = subresource_label(r) {
                    writeln!(
                        dot,
                        "    {res_id} -> pass_{i} [style=solid, color=\"#7fb3d8\", \
                         label=\"{label}\"];",
                    )
                    .unwrap();
                } else {
                    writeln!(
                        dot,
                        "    {res_id} -> pass_{i} [style=solid, color=\"#7fb3d8\"];",
                    )
                    .unwrap();
                }
            }
            for w in &pass.writes {
                let res_id = resource_node_id(w);
                if let Some(label) = subresource_label(w) {
                    writeln!(
                        dot,
                        "    pass_{i} -> {res_id} [style=bold, color=\"#e8a87c\", \
                         label=\"{label}\"];",
                    )
                    .unwrap();
                } else {
                    writeln!(
                        dot,
                        "    pass_{i} -> {res_id} [style=bold, color=\"#e8a87c\"];",
                    )
                    .unwrap();
                }
            }
        }

        writeln!(dot, "}}").unwrap();
        dot
    }
}
