# eui-neo-wgpu

`eui-neo-wgpu` contains wgpu renderer support for `eui-neo`.

The crate consumes `eui_neo::Frame` and renders it into a host-provided wgpu
command encoder and target view. It owns the Neo GPU pipelines, shader sources,
text rendering, image cache, backdrop capture path, and renderer-local wgpu
resources. Hosts pass raw wgpu frame parts through `Target` and resolve images
through `Resources`; engine-specific frame lifecycles stay outside this crate.
