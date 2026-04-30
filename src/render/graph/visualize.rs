//! GraphViz DOT export for render graph visualization.

use std::fmt::Write;

use super::*;

impl RenderGraph {
    /// Export the render graph as a GraphViz DOT string.
    ///
    /// For accurate alive/culled state, call [`compile`] first.
    pub fn export_dot(&self) -> String {
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
            writeln!(
                dot,
                "    res_tex_{i} [label=\"{name}\\n{size:?} {format:?}\", \
                 shape=box, style=filled, fillcolor=\"#2d2d44\", color=\"#6c6c8a\"];",
                name = dot_escape(&tex.name),
                size = tex.size,
                format = tex.format,
            )
            .unwrap();
        }
        for (i, buf) in self.buffers.iter().enumerate() {
            writeln!(
                dot,
                "    res_buf_{i} [label=\"{name}\\n{size} bytes\", \
                 shape=box, style=filled, fillcolor=\"#243447\", color=\"#4f81bd\"];",
                name = dot_escape(&buf.name),
                size = buf.size_bytes,
            )
            .unwrap();
        }
        writeln!(
            dot,
            "    res_surface [label=\"Surface\", shape=box, style=filled, \
             fillcolor=\"#44223d\", color=\"#8a4477\"];"
        )
        .unwrap();
        writeln!(dot).unwrap();

        // Pass nodes
        writeln!(dot, "    // Passes").unwrap();
        for (i, pass) in self.passes.iter().enumerate() {
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
                "    pass_{i} [label=\"{name}\\n({ty:?})\", \
                 shape=ellipse, style=\"{style}\", fillcolor={fill}, color={border}];",
                name = dot_escape(&pass.name),
                ty = pass.pass_type,
            )
            .unwrap();
        }
        writeln!(dot).unwrap();

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
