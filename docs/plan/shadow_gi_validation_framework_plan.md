# Shadow And GI Validation Framework Plan

## Purpose

这份计划的目标不是继续“凭眼睛调阴影”，而是先搭一个能验证的基础框架。

当前 three_d_demo 的问题已经不再是单个参数能解释的简单 bug。屏幕上看到的“阴影割裂、突变、室内暗部不一致、F6 调试视图和最终画面不一致”，可能来自多层叠加：

- 直射光阴影：directional CSM、atlas、cascade split、bias、PCF/PCSS。
- 近距离补充阴影：contact shadows。
- 间接光和环境暗部：SSGI / GI composite / ambient。
- 后处理历史：TAA、temporal accumulation、denoise。
- 材质路径差异：standard material 和 normal mapped material 的 shader 分支。

所以正确路线是：

1. 先把每一层拆开看。
2. 再用 CPU 公式测试证明数学没有错。
3. 再用最小 GPU 场景读回像素证明 shader 和 render pass 没有错。
4. 最后才在 three_d_demo 里调真实场景。

这份计划和 `docs/plan/wicked_shadow_gap_closure_plan.md`、`docs/plan/shadow_system_direct_rewrite_plan.md` 是互补关系：

- `wicked_shadow_gap_closure_plan.md` 负责说明 SkyEngine 和 WickedEngine 的功能差距。
- `shadow_system_direct_rewrite_plan.md` 负责长期架构重写方向。
- 本文件负责短中期的验证框架和排查流程，避免每次修改都变成“看起来好了，但不知道为什么”。

## Current Baseline

已经完成或正在使用的基础能力：

- three_d_demo 已有阴影和光照隔离开关：
  - `F6` 到 `F11`：CSM / cascade / debug 视图。
  - `F12`：只看 direct lighting。
  - `G`：只看 indirect lighting。
  - `C`：开关 contact shadows。
  - `V`：开关 GI。
- `RenderDebugView` 已有 direct-only 和 indirect-only 路径。
- material shader 已经能跳过 GI composite 和 contact shadows，用于观察直射光阴影。
- 已添加 test-only 公式验证：
  - `src/render/lighting/shadow/formula.rs`
  - 覆盖 cascade edge fade、cascade index、PCSS radius、penumbra、receiver depth clamp、blocker search 等。
- 已增强 GPU readback 阴影测试：
  - `src/render/runtime/tests/shadows.rs`
  - 验证最小场景中被遮挡区域变暗，同时远处 receiver 保持亮。

当前已验证命令：

```text
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::runtime::tests::shadows
cargo check --examples --features app
```

## Principles

### 1. 不再裸调参数

以后改这些内容前，必须先说明对应的验证点：

- cascade split / blend。
- shadow view fitting。
- atlas scale/add。
- raster depth bias。
- receiver compare bias。
- normal bias。
- PCF / PCSS radius。
- blocker search。
- contact shadow thickness / falloff。
- GI composite strength。

如果只是“把 bias 调大一点看看”，那最多只能作为临时诊断，不应该直接留下。

### 2. 先分层，再判断

最终颜色大致可以理解成：

```text
final color =
  material base
  * direct lighting shadow
  + indirect lighting / GI
  + contact shadow modulation
  + post processing / temporal history
```

用户肉眼看到的“阴影不对”，不一定是 CSM 不对。必须先用 debug 模式拆开：

- `F12` 后还不对：优先查 direct shadow / CSM / bias / PCSS。
- `G` 后才明显不对：优先查 indirect lighting / GI。
- 关 `C` 后问题消失：优先查 contact shadows。
- 关 `V` 后问题消失：优先查 GI composite。
- debug cascade 视图正常、最终画面突变：优先查 shader 合成或后处理。

### 3. Wicked 是参考，不是复制粘贴

对齐 WickedEngine 的方式应该是：

- 对齐算法意图。
- 对齐关键公式。
- 对齐边界条件。
- 对齐 debug 能力。
- 对齐稳定性策略。

不应该做：

- 直接照搬 HLSL 到 WGSL。
- 忽略 wgpu clip-space、坐标约定、bind group 结构差异。
- 把 Wicked 的 renderer 全量架构硬塞进 SkyEngine。

### 4. 测试分三层

验证框架分三层：

- CPU formula tests：验证公式、边界和单位。
- GPU minimal scene tests：验证 shader、pass、texture、binding、depth compare。
- Demo diagnosis workflow：验证真实场景、真实后处理和用户可见效果。

三层不能互相替代。CPU 测试过了，不代表 shader 采样没错；demo 看起来对了，也不代表数学稳定。

## Wicked Alignment Targets

### Direct Shadow Sampling

主要参考：

- `_refs/WickedEngine/WickedEngine/shaders/lightingHF.hlsli`
- `_refs/WickedEngine/WickedEngine/shaders/shadowHF.hlsli`

需要持续对齐的点：

- cascade 选择：边界行为要明确，是 `>` split 还是 `>=` split。
- cascade edge fade：blend 宽度和边缘衰减曲线要可测试。
- atlas border clamp：采样不能越过 cascade rect。
- PCF / PCSS 半径：radius 从艺术参数到 texel 半径的映射要固定。
- blocker search：无 blocker、全遮挡、部分遮挡都要有测试。
- penumbra：receiver 和 blocker 的深度差如何变成软阴影半径。
- receiver plane depth bias：避免 acne，同时不能把接触阴影完全抹掉。

### Shadow Camera And Caster Contract

主要参考：

- WickedEngine directional shadow camera creation。
- `src/render/lighting/shadow/view.rs`。

需要对齐的点：

- cascade frustum corners 是否完整包含当前 split。
- light view basis 是否稳定，不因光照方向接近竖直而翻转。
- texel snapping 是否只稳定中心，不缩小覆盖范围。
- receiver depth extent 和 caster depth extent 是否分开。
- caster 能否在不属于当前 camera split 的位置，仍然向该 split 投影。
- near cascade 是否会被 far caster 过度拉大深度范围。

### Debug Surface

Wicked 的强点不是单个公式，而是很多中间状态能看见。SkyEngine 需要补齐：

- cascade coverage。
- raw shadow atlas。
- per-cascade caster count。
- per-cascade split near/far。
- per-cascade texel world size。
- direct-only / indirect-only。
- contact shadow mask。
- GI-only / GI-off。
- 可选 readback dump。

## Milestone 0: Freeze Repro

### Goal

固定一个大家都认的 three_d_demo 问题场景，避免每次看到的是不同角度、不同开关、不同历史帧。

### Tasks

- 在文档中记录一个 canonical repro：
  - demo 名称。
  - camera 位置 / yaw / pitch / distance。
  - light direction。
  - GI 是否开启。
  - contact shadows 是否开启。
  - TAA 是否开启。
  - 当前 debug view。
- 如果 demo 目前没有稳定相机 preset，添加一个 debug-only preset。
- 记录“坏现象”属于哪类：
  - cascade 边界突变。
  - 室内没有直接阴影。
  - 室内 indirect 过暗或过亮。
  - contact shadow 条纹。
  - PCSS 软化导致边界漂浮。
  - temporal history 残影。

### Files

- `examples/render/three_d_demo.rs`
- `docs/plan/shadow_gi_validation_framework_plan.md`
- optional: `docs/render_deep_dive.md`

### Acceptance

- 有一个固定复现场景。
- 后续修改都能回到这个场景比较。
- 不再用“我刚才那个角度”作为唯一复现方式。

## Milestone 1: Debug Layer Isolation

### Goal

把最终画面拆成 direct、indirect、contact、GI、post-process 几层，让问题先归类。

### Tasks

- 确认 `F12` direct-only 完全跳过：
  - GI composite。
  - SSGI。
  - contact shadows。
  - indirect ambient contribution。
- 确认 `G` indirect-only 完全跳过：
  - direct light diffuse/specular。
  - direct shadow factor。
  - direct contact shadow。
- 确认 `C` 能明确开关 contact shadows，不影响 CSM。
- 确认 `V` 能明确开关 GI，不影响 direct shadow。
- 给 debug HUD 或日志补充当前开关状态：
  - debug view。
  - GI on/off。
  - contact shadows on/off。
  - shadow sampling mode。
  - cascade count。

### Files

- `examples/render/three_d_demo.rs`
- `src/render/builtins/debug.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/postfx/contact_shadows.rs`
- `src/render/gi/`

### Acceptance

- 同一视角下能回答：“这个暗块来自 direct shadow 还是 indirect/GI”。
- 如果 `F12` 画面正确而默认画面错误，下一步不会再查 CSM。
- 如果 `F12` 画面错误，下一步优先查 CSM / shader compare。

### Commands

```text
cargo check --examples --features app
cargo test --features app render::builtins::debug
cargo test --features app render::postfx::contact_shadows
```

## Milestone 2: CPU Formula Contract

### Goal

把从 Wicked 翻译过来的公式变成可测试契约，避免“看起来像 Wicked，但边界不一样”。

### Current Files

- `src/render/lighting/shadow/formula.rs`
- `src/render/lighting/shadow/mod.rs`

### Existing Coverage

- cascade edge fade。
- cascade index 边界。
- cascade count clamp。
- PCSS radius remap。
- PCSS penumbra clamp。
- receiver plane compare depth clamp。
- blocker search no/full occlusion。
- material compare bias 当前禁用。
- receiver bias pressure。

### Next Tasks

- 增加 Wicked fixture table：
  - 输入 radius，输出 sample radius。
  - 输入 receiver/blocker depth，输出 penumbra。
  - 输入 split depths，输出 cascade index。
  - 输入 edge blend width，输出 fade。
- 把每个 fixture 注明来源：
  - 来自 Wicked 公式。
  - 来自 SkyEngine 坐标约定转换。
  - 来自有意不同的设计。
- 增加负例测试：
  - `>= split` 会导致边界选择改变。
  - radius 没有 remap 会导致 PCSS 明显过窄。
  - compare depth 没 clamp 会越界。

### Acceptance

- 所有阴影核心公式都有测试。
- 任何人改公式时，失败信息能说明是哪个物理意义变了。
- 对齐 Wicked 的地方明确写在测试名或注释里。

### Commands

```text
cargo test --features app render::lighting::shadow::formula
```

## Milestone 3: Minimal GPU Shadow Scenes

### Goal

证明 GPU 实际渲染路径和 CPU 公式一致。

CPU 公式正确，不代表 GPU 正确。GPU 还可能错在：

- uniform packing。
- bind group 绑定。
- WGSL 分支。
- texture compare。
- depth format。
- atlas rect。
- render pass clear。
- camera matrix。
- material receive/cast flags。

### Current Files

- `src/render/runtime/tests/shadows.rs`

### Existing Coverage

- 一个最小 standard material 场景中，receiver 被 directional shadow 正确压暗。
- 同一测试中，远处区域保持亮，防止整张图被错误变暗。

### Next Tasks

- 新增 cascade boundary GPU test：
  - 两个 receiver 分别落在 split 两侧。
  - 验证没有突然全黑/全亮。
  - 验证 edge blend 区域过渡合理。
- 新增 caster outside receiver split test：
  - caster 不在当前 camera split 内。
  - 但沿 light direction 可以投影到 receiver。
  - 验证它不会被错误 cull 掉。
- 新增 no-shadow-control test：
  - 关闭 cast_shadow 后 receiver 变亮。
  - 关闭 receive_shadow 后 receiver 不再被压暗。
- 新增 PCF vs PCSS stability test：
  - 固定同一场景。
  - PCF 和 PCSS 都不能出现整区突变。
  - PCSS 允许软化，但不能吞掉接触关系。

### Files

- `src/render/runtime/tests/shadows.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`

### Acceptance

- GPU readback 不只看单个像素，而是优先看小区域统计：
  - min。
  - max。
  - average。
  - lit region vs shadow region ratio。
- 每个测试都有明确“应该亮”和“应该暗”的区域。
- 测试失败时能区分“没有影子”和“整片错误变暗”。

### Commands

```text
cargo test --features app render::runtime::tests::shadows
```

## Milestone 4: Cascade Fitting Validation

### Goal

验证 shadow camera 的几何合同，而不是直接看最终阴影。

这是当前最容易出错、也最容易造成“割裂”和“突变”的部分。

### Tasks

- 给 `build_shadow_view()` 或其纯 CPU 子函数补测试。
- 验证每个 cascade split 的 8 个 frustum corners 都在 shadow projection 内。
- 验证 texel snapping 后仍然包含原始 corners。
- 验证 light direction 接近竖直时 basis 不翻转。
- 验证 camera 小幅旋转时：
  - shadow extent 不乱跳。
  - texel world size 稳定。
  - cascade center snapping 是阶梯式小跳，不是大范围抖动。
- 验证 split near/far 和 shader cascade selection 使用同一套距离定义。

### Files

- `src/render/lighting/shadow/view.rs`
- optional: `src/render/lighting/shadow/plan.rs`

### Acceptance

- 测试能直接回答：
  - receiver 有没有被 projection 裁掉。
  - cascade 边界有没有因为 CPU 和 shader 约定不一致而错位。
  - snapping 有没有缩小覆盖范围。

### Commands

```text
cargo test --features app render::lighting::shadow::view
```

## Milestone 5: GI And Contact Shadow Isolation

### Goal

确认“直射光阴影”和“室内阴影/暗部”不是混在一起判断。

用户已经观察到：直射光阴影和室内阴影不一样，问题可能和 GI 有关。这个判断是合理的。室内暗部通常更多来自 indirect/GI/ambient occlusion，不一定来自 directional shadow map。

### Tasks

- 给 GI composite 增加最小 readback 测试：
  - GI off：只看 direct。
  - GI on：间接项按预期增加或改变。
  - debug indirect-only：不包含 direct shadow。
- 给 contact shadow 增加 mask readback 或统计测试：
  - 平面和物体接触处变暗。
  - 非接触区域不产生条纹。
  - depth discontinuity 附近有 fade，而不是硬断。
- 确认 contact shadows 不会在 direct-only debug 中参与。
- 确认 GI 不会在 direct-only debug 中参与。
- 确认 indirect-only 下 direct shadow factor 不参与。

### Files

- `src/render/gi/`
- `src/render/postfx/contact_shadows.rs`
- `src/render/shaders/postfx/contact_shadows.wgsl`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/runtime/tests/`

### Acceptance

- 如果室内暗部不对，测试和 debug 能明确把责任指向 GI/contact/ambient。
- 不再把室内没有“直射影子”误判成 CSM 失败。
- direct-only 和 indirect-only 的定义在 shader 和测试里一致。

### Commands

```text
cargo test --features app render::postfx::contact_shadows
cargo test --features app render::runtime::tests
```

## Milestone 6: Optional Debug Dumps

### Goal

当肉眼判断不清时，可以把中间结果导出，不靠截图猜。

### Tasks

- 增加环境变量控制的 debug dump，默认关闭：
  - `SKY_SHADOW_DUMP=1`
  - `SKY_SHADOW_DUMP_DIR=...`
- 可 dump 内容：
  - final color。
  - direct-only。
  - indirect-only。
  - contact shadow mask。
  - raw shadow atlas。
  - cascade coverage。
- 文件名包含：
  - frame index。
  - debug view。
  - camera preset。
  - light id / cascade index。
- readback 使用异步 map，避免常规帧路径强制 stall。

### Files

- `examples/render/three_d_demo.rs`
- `src/render/runtime/`
- optional: `src/render/lighting/shadow/debug.rs`

### Acceptance

- 不开环境变量时没有额外开销或只有极小分支开销。
- 开启后能拿到可比较的 png 或 raw dump。
- dump 文件能用于 bug 报告和回归对比。

## Milestone 7: Demo Acceptance Checklist

### Goal

把 three_d_demo 的“看起来对不对”变成固定检查表。

### Checklist

每次阴影/GI 关键修改后，至少检查：

- 默认画面：
  - 室外物体有稳定投影。
  - 室内暗部合理，不因 direct shadow 缺失而错误全黑。
  - camera orbit 时没有明显 cascade 突变。
- `F12` direct-only：
  - 只显示直射光和 CSM 结果。
  - 室内没有直射光处可以暗，但不能混入 GI/contact 的额外变化。
- `G` indirect-only：
  - 不应出现 directional shadow 的硬边界。
  - 应主要反映 GI/ambient。
- `C` contact off：
  - 接触处暗化变少。
  - 大面积 CSM 阴影不应消失。
- `V` GI off：
  - 间接亮度变化明显。
  - direct shadow 边界不应大幅改变。
- `F6` 到 `F11`：
  - cascade coverage 与 final direct shadow 的突变位置能解释得上。
  - raw cascade 没有明显被裁掉的 caster/receiver。

### Acceptance

- 如果默认画面不对，能在 5 分钟内定位到 direct、indirect、contact、GI、post-process 中的一个层。
- 如果不能定位，说明 debug surface 不够，需要先增强 debug，而不是继续调参。

## Milestone 8: Regression Gate

### Goal

把验证命令固定下来，后面每次“继续对齐”都跑同一组。

### Required Before Merging Shadow/GI Changes

```text
cargo fmt
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::lighting::shadow::view
cargo test --features app render::runtime::tests::shadows
cargo test --features app render::postfx::contact_shadows
cargo check --examples --features app
```

### Optional Wider Run

```text
cargo test --features app
cargo check --examples --features app
```

### Acceptance

- 公式层、GPU 最小场景层、demo 编译层全部通过。
- 若某个测试因为环境 GPU 问题不能跑，必须在汇报中说明，不假装已经验证。

## Immediate Next Work

建议下一步按这个顺序做：

1. 补 `view.rs` 的 cascade fitting 测试。
2. 补 `shadows.rs` 的 cascade boundary GPU readback 测试。
3. 补 Wicked fixture table，让公式测试更像“对照表”。
4. 给 three_d_demo 加固定 debug camera preset。
5. 给 debug HUD 或日志显示当前 direct/indirect/contact/GI 状态。
6. 再回到真实画面，判断突变到底来自 CSM 还是 GI/contact。

不要先做：

- 继续加大 bias。
- 继续换 PCSS 参数。
- 直接重写整套 shadow phase。
- 把 GI 和 direct shadow 混在同一个 shader 分支里猜。

## Risk Notes

- GPU readback 测试可能受设备、后端、浮点差异影响，阈值要用区域统计，不能只盯一个像素。
- `cargo check --examples --features app` 只能证明编译，不证明 WGSL 所有运行路径正确。
- PCSS 会掩盖一部分 CSM 错误，所以诊断时先用更确定的 PCF 或硬阴影。
- contact shadow 很容易制造“像阴影贴图错了”的条纹，必须能单独关掉和单独看。
- 室内“没有直射阴影”很多时候是正常的，因为 direct light 没照进去；室内暗部主要应由 GI/ambient/occlusion 解释。
- Wicked 和 SkyEngine 的坐标、clip space、资源绑定不同，公式能对齐，代码形状不必一样。

## Definition Of Done

这个验证框架完成后，应满足：

- 公式正确性有 CPU tests。
- shader/render pass 正确性有 GPU readback tests。
- three_d_demo 有固定复现和分层 debug 流程。
- direct shadow、indirect/GI、contact shadow 能被分别观察。
- 和 Wicked 对齐的地方有 fixture 或注释说明。
- 每次改阴影/GI 都有固定回归命令。
- 当用户说“还是不太对”时，下一步不是猜，而是按 debug 层快速定位。
