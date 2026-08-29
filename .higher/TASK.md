# HAVEN VISUAL & WORLD POLISH ALPHA v3.0
# 视觉、世界、声音与第一印象完整打磨版 —— 一次性连续施工任务书

项目路径：`C:\Users\37653\Desktop\Haven`

当前安全基线：
- branch: `main`
- checkpoint: `316ebea`
- baseline: FIRST CONTINENT PLAYABLE ALPHA v2.0
- Godot 4.7.x + GDScript + Compatibility

## 0. 本任务的唯一目标

这不是加新系统的版本，而是把 Haven 从“功能丰富但像开发测试场景”提升成“第一次真正像一款有自己气质、能截图给别人看的 2D 像素开放世界游戏”。

完成后，玩家第一次进入 Haven 应立即感受到：
- 角色、房屋、树、道路、水与地形都有明确视觉身份；
- 新手村像真实有人生活的地方；
- 森林、深林、沼泽、农田、城市、港口、海岸、高地、遗迹一眼可区分；
- 海边不是蓝色地图墙；
- MiniMap 不是右上角巨大黑框；
- 环境声音不是电子“滴滴滴”；
- 昼夜、雨、雾、季节真正改变氛围；
- 玩家能靠 Landmark 和环境自然找方向；
- 无任务箭头、无强制教程，仍会产生“我想去那边看看”的冲动。

## 1. 执行规则

开始前依次完整阅读：
1. `.haven/HAVEN_CHARTER.md`
2. 当前 `.haven/TASK.md`
3. `.haven/HAVEN_DESIGN.md`
4. `.haven/HAVEN_VISUAL_BIBLE.md`
5. 当前源码、资源、数据、测试

然后：
- `git status`
- `git log -1 --oneline`
- 记录 TASK.md SHA256
- 记录 HAVEN_CHARTER.md SHA256

随后连续施工：
`审计 → 设计 → 资产生成/修改 → 集成 → 运行 → 截图 → 检查 → 修复 → 全量回归 → 文档 → 最终报告`

正常情况下不要中途停下来询问负责人。仅以下情况允许暂停：
- Charter 与 TASK 有无法化解的产品冲突；
- 需要删除用户真实数据；
- 需要更换 Godot/GDScript 技术栈；
- 需要使用版权/来源无法确认的外部素材；
- 会不可逆破坏 Save / stable ID / 世界坐标；
- 引擎级阻塞无法自行解决。

## 2. TASK / CHARTER 文件保护

`.haven/TASK.md`：
- 只读；
- 禁止修改、追加、格式化、写进度；
- 开始/结束 SHA256 必须一致。

`.haven/HAVEN_CHARTER.md`：
- 永久宪法；
- 禁止修改；
- 开始/结束 SHA256 必须一致。

本轮完成后允许更新：
- `.haven/HAVEN_DESIGN.md`
- `.haven/HAVEN_VISUAL_BIBLE.md`
- `README.md`

## 3. 当前功能基线必须全部保留

禁止因视觉重做丢失：
- 960×540 logical viewport；
- 1920×1080 window override；
- integer scaling / viewport stretch；
- F11 fullscreen；
- Title/New Game/Continue/Settings/Exit；
- 第一大陆 13 Region；
- Starter Village / City / Port / Coast / Lake / Marsh / Highlands / Quarry / Mine / Ruins / Islands；
- Player / Camera / Roof Fade / Hut Egress；
- Inventory / 6-slot Hotbar；
- 38 items / 10 recipes / 3 crops / 7 fish；
- 12 NPC / Schedule / Relationship；
- Economy / Crafting / Farming / Fishing；
- Boat / Combat / 4 enemy types；
- Day/Night / Weather / Seasons；
- Echo / Dash / Home Upgrade；
- MiniMap / Fog of War / Full Map / Journal；
- Save v4 + v1→v4 migration；
- baked first continent；
- chunk activation；
- 现有 14 套、369 项回归测试。

## 4. Scope Freeze

本轮核心：
**视觉、世界密度、UI、Camera、Lighting、VFX、Audio、Loading 感知、环境叙事。**

本轮禁止新增：
- 第二大陆；
- 完整 Quest System；
- 主线剧情；
- 大量新 NPC；
- Romance / Marriage；
- 大型 Skill Tree；
- 新职业；
- 大量新武器；
- Boss 系统扩张；
- Multiplayer / Online / Cloud Save；
- Mod 系统；
- 无限世界。

已有系统允许补反馈，不新增大型玩法体系。

## 5. 视觉设计最高原则

1. **Readability first**：角色/敌人/交互物/地标必须先看得懂，再谈漂亮。
2. **Silhouette**：角色、树、建筑、地标必须有明确轮廓。
3. **Contrast**：重要对象与背景分离，但不靠巨大箭头、发光圈、任务标记。
4. **Landmark navigation**：通过高度、轮廓、色彩、运动、光、烟、风车、灯塔、山峰等自然吸引视线。
5. **Environmental storytelling**：用破车、旧营地、散落货物、断桥、废弃渔具、侵蚀遗迹等讲故事，不用文字直接解释。
6. **Pixel detail**：像素画不是“块越大越像素”，而是细小像素细节 + 清晰轮廓 + 统一比例。
7. **原创**：不得复制 Stardew/Terraria/Dead Cells/Minecraft 等现成素材。

## 6. Pixel Pipeline

优先保留 `960×540`：
- Stretch Mode = `viewport`
- Aspect = `keep`
- Scale Mode = `integer`
- Default filter = Nearest
- Pixel art textures 默认不使用 mipmap

评估：
`rendering/2d/snap/snap_2d_transforms_to_pixel = true`

不要同时默认开启 transform snap + vertex snap。

若 pixel snap 导致明显移动抖动：
- 关闭 Camera smoothing；
- Camera 最终位置对齐 pixel grid；
- 物理仍可 float；
- Sprite 视觉对齐整数像素。

### Camera
960×540 下默认 zoom 改为 `1.0`。

滚轮分档保留：
- `0.5` Far
- `1.0` Default
- `2.0` Close

禁止连续 fractional zoom tween。
HUD 不随 Camera 缩放。

## 7. Haven Visual Identity v3

整体：
**温暖、自然、安静、有生活痕迹；未知区域冷、孤独、神秘。**

文明区：
- 暖木色
- 土色
- 柔和灯光
- 烟
- 生活杂物

Ancient/Echo：
- 冷石
- 青蓝微光
- 稀薄雾
- 慢粒子
- 更空旷、更安静

禁止：
- 糖果高饱和；
- 全世界阴暗；
- 大面积纯色；
- 直接模仿某一款现成游戏。

## 8. 统一 Palette

建立 `data/visual_palette.json` 或等价统一资源，至少包含：
- Neutral：ink/deep shadow/stone/warm light/UI cream
- Nature：grass light/base/shadow、moss、leaf light/dark、dry grass
- Earth：dirt、mud、wet earth、wood light/base/dark
- Water：shallow/river/lake/ocean/deep ocean/foam
- Mystery：Echo cyan/blue、ancient stone/shadow
- Season accents：spring/summer/autumn/winter

核心色控制约 32～48 色，允许灯光/tint动态混合。
禁止每个脚本随意硬编码完全不同的颜色。

## 9. Pixel Density

Tile 仍为 32×32 logical grid，但 Tile 内必须真正使用 1px/2px 细节：
- 草叶 cluster
- 泥土纹理
- 小石
- 边缘
- 阴影
- 花
- 根
- 湿痕
- 颗粒

禁止：
- 32×32 单一颜色；
- 纯色矩形屋顶；
- 纯矩形树冠；
- 纯六边形石头；
- 每像素随机电视雪花噪声。

噪声必须 deterministic、cluster-based、低频、有结构。

## 10. Player v3

视觉 Sprite 建议升级至约 `24×32` logical px/frame（允许 22×30～28×36 微调），碰撞保持脚底逻辑。

必须有：
- hair silhouette
- face
- torso
- arms/hands
- pants
- shoes
- 1px face-direction detail
- ground shadow

颜色至少：
- hair highlight/base/shadow
- skin light/base/shadow
- shirt light/base/shadow
- pants base/shadow

动画：
- 4-dir idle
- 4-dir walk，至少4帧
- swing
- hurt
- dash
- idle 可做极轻微呼吸/手部1px变化

Y-sort origin 位于脚底。

## 11. NPC v3

12 个 NPC 不允许只是“同一小人换4个色”。

最低组合库：
- 4 body silhouettes
- 6 hair silhouettes
- 6 clothing palettes
- 2～3 posture/age differences

职业可通过衣服/轮廓暗示：
- blacksmith
- farmer
- merchant
- carpenter
- scholar/relic
等。

禁止巨大职业图标。

## 12. Trees v3

至少：
1. Broadleaf
2. Forest Dense
3. Deep Forest Ancient
4. Dead Tree
5. Marsh Tree
6. Highlands Pine
7. Lake Willow

普通树约 40×56 ～ 56×72，可按画面微调。

树必须有：
- irregular crown
- 2～4 crown colors
- visible trunk
- root hint
- contact shadow
- broken edge silhouette

玩家走到大树背后：
只淡化遮挡人物的 canopy，不整棵消失。

## 13. Rocks / Ore v3

至少：
- pebble
- normal rock
- large rock
- cracked rock
- copper ore
- iron ore
- crystal node
- cliff rock
- coastal rock

Cracked Rock 不靠文字也看得出“可破坏”。
Ore 使用局部色彩点缀，不把整块石头染成纯矿色。

## 14. Building v3 —— P0

当前“纯几何建筑”必须退出主要画面。

建立 modular pixel building system：
- roof
- roof edge
- wall
- beam
- door
- window
- foundation
- shadow
- chimney/sign/detail

禁止 `一个大 Polygon2D = 房子`。

尺寸建议：
- Player Hut：4×3 或 4×4 tile
- Small House：4×4 / 5×4
- Shop：5×4 / 6×5
- Blacksmith：6×5
- Tavern：7×5 / 8×6

### Player Hut
必须一眼表现“村里最破旧的住所”：
- patchy thatch
- irregular roof edge
- exposed beam
- crooked door
- tiny window
- patched wall
- old crate/firewood
- uneven foundation

### Roof Fade
普通小建筑继续同地图内部：
- roof alpha 约 0.05～0.12
- front wall alpha 约 0.20～0.32
- 不影响碰撞
- 进出正常

## 15. Interiors v3

可进入的小型建筑模板都做视觉装修。

基础家具：
- bed
- table
- chair
- shelf
- chest
- stove/fireplace
- rug
- barrel/crate
- wall detail
- work props

Blacksmith：
forge/anvil/coal/tool rack/metal table

Shop：
shelves/crates/counter/sign

Tavern：
tables/counter/warm light/fireplace

禁止所有室内只是“同一套家具复制”。

## 16. Terrain Layers

保持 TileMapLayer 架构，明确职责：
1. GroundBase
2. GroundTransition
3. GroundDetail
4. Water
5. StructureGround
6. AboveGround/Canopy

动态 Y-sort entity 独立。

Static terrain Bake 后尽量不再大面积 runtime 修改。
不要频繁调用强制 TileMap update。

## 17. Ground Tiles v3

至少升级：
- grass
- meadow grass
- forest floor
- deep forest floor
- dirt
- wet dirt
- road
- gravel
- farmland
- tilled soil
- sand
- wet sand
- marsh mud
- stone paving
- quarry rock
- highland ground
- ancient floor

每种：
- base
- 2～4 variation
- edge/corner transition

## 18. Terrain Transition

禁止垂直/水平硬矩形分界。

Forest→Grass：
散树→灌木→草影→草原

Marsh→Grass：
湿草→泥→芦苇→浅水

Coast→Grass：
草→dry grass→sand→wet sand→foam→shallow water

使用：
- irregular edge
- corners
- tufts
- rocks
- flowers
- roots
- transition band

## 19. Road v3

道路不能再像32px方格蛇。

需要：
- width variation
- irregular edges
- grass intrusion
- wheel rut/footprint
- occasional stone
- natural curves
- forks
- bridge approach

主路清楚，小路更窄更自然。

## 20. Water v3

保留已有水动画技术，重做视觉。

区分：
- river
- lake
- shallow
- ocean
- deep ocean
- marsh water

River：
方向感 ripple、岸边、芦苇、桥阴影

Lake：
安静、小波纹、lily/reed、夜间微光

Ocean：
`Deep Sea → Sea → Shallow → Foam → Wet Sand → Dry Sand → Land`

海岸禁止直线。

## 21. Coast v3 —— 视觉名片

South Coast / Port 重点打磨：
- irregular beach
- coves
- rocky coast
- cliff
- foam
- tide pool
- seaweed
- shell
- driftwood
- rock cluster
- dock
- shipwreck
- sea cave entrance

至少3种不同海岸构图：
1. 沙滩海湾
2. 礁石海岸
3. 悬崖/港湾

两个离岸岛也要有视觉差异。

## 22. Lighting v3

Day/Night 不再只靠全屏 tint。

保留 CanvasModulate，增加有限的 2D lights。

Day：
环境色为主，不到处 PointLight。

Dusk：
暖色过渡、窗灯逐步亮。

Night：
只在以下位置使用必要 PointLight2D：
- hut fire
- windows
- lanterns
- forge
- campfire
- lighthouse
- ancient device
- Echo

大多数树/石使用预绘 contact shadow。
不要给全世界树开启昂贵实时阴影。

## 23. Weather v3

补齐明确的：
1. Clear
2. Rain
3. Fog

Rain：
- 高效粒子
- 不挡视野
- 落水少量 ripple
- rain ambience
- transition 不是瞬切

Fog：
- 低频移动
- 不均匀
- Marsh 更明显
- Coast early morning 可出现
- Highlands 可薄雾
- 不用纯白半透明大矩形

天气切换建议渐变5～20秒。

## 24. Seasons v3

四季不能只有 tint。

Spring：
花、嫩绿细节

Summer：
深绿、茂密植被

Autumn：
金/橙叶片、稀疏落叶

Winter：
霜、苍白草、部分雪块、冷水色、部分裸枝

不复制四套大陆。
使用 palette + detail overlay + vegetation variants + particles。

## 25. Starter Village v3 —— 第一印象最高优先级

保持“不新手的新手村”。

至少：
- Player Hut
- 5+ distinct houses
- Blacksmith
- General Shop
- Carpenter/Workshop
- Tavern/Inn
- Well
- Small square
- Garden plots
- animal yard visual
- firewood
- fence
- barrel/crate
- signs
- flowers
- trees/shrubs
- benches/stumps

道路自然连接 Forest/Farmland/Valley。
村庄禁止棋盘布局感。

每栋房外至少2～4个“生活用途”小物件。
NPC 可增加轻微 idle turn / 小范围 wander，但不重写复杂AI。

## 26. Central Valley v3

开阔但不空：
- meandering road
- river
- bridge
- wildflower meadow
- lone tree cluster
- stone marker
- broken cart
- camp trace
- view toward mountain/city landmark

每8～15秒移动应遇到新构图。

## 27. West Forest v3

- layered canopy
- ground detail
- roots
- fallen logs
- mushrooms
- fern
- berry
- narrow trail
- leaf/bird ambience

至少6个环境 composition。

## 28. Deep Forest v3

区别于普通森林：
- darker floor
- bigger trees
- narrower sightline
- ancient roots
- mist pockets
- firefly/Echo motes at night
- 更少人工道路

Secrets 必须有环境 staging，不只是角落放物品。

## 29. Southwest Marsh v3

- dark mud
- shallow black/green water
- reed
- dead tree
- broken boardwalk
- bubbles
- fog
- moss
- old stone
- half-sunken object

音频必须自然：
water/frog/insect/reed/wind。
禁止电子蜂鸣虫叫。

## 30. East Farmland v3

- patchwork fields
- fences
- irrigation
- hay
- barn
- windmill
- orchard
- farm road
- scarecrow

Windmill 要有轻微动画，作为 Landmark。

## 31. Quarry v3

- terraced stone
- cut rock face
- gravel
- cart
- rails/wood beams
- tool debris
- dust
- ore hints

Pickaxe progression 保留。
Mine entrance 清晰但不使用箭头。

## 32. East City v3

城市不能只是“15栋房壳”。

至少强化：
- main gate
- main street
- market square
- bell/clock tower
- residential lane
- service alley
- merchant area
- port road

建筑差异：
- roof shape/color
- wall material
- sign
- awning
- windows
- footprint

街道物件：
stall/crate/barrel/cart/lamp/bench/awning/flowers/notice board

已有功能建筑必须有内部视觉：
shop/tavern/tool shop/carpenter/relic-scholar。

## 33. Port v3

必须成为最强 Landmark 之一：
- lighthouse
- docks
- warehouse
- fish market
- boat
- rope
- bollard
- crate
- net
- barrel
- reflections
- foam
- gull-like environmental motion

Lighthouse 夜晚有扫光/旋转光的视觉印象。

## 34. Lake District v3

- irregular shoreline
- reeds
- lily
- willow
- small dock
- reflection hint
- island
- quiet water

Night Lake Secret：
通过微光、Echo motes、环境声变化表达。
禁止弹“秘密已解锁”。

## 35. North Highlands v3

- rock layers
- pine
- cliffs
- highland grass
- wind particles
- sparse vegetation
- seasonal frost
- mountain landmark

声音中风更明显。

## 36. Ancient Region / Ruins v3

视觉语言：
- large monolith
- cool stone
- geometric carving
- moss
- cyan/teal accent
- faint particles
- 更大的负空间

Ruins interior：
- patterned floor
- broken walls
- shadow
- mechanisms
- Guardian area
- hidden room
- Ancient Core

Echo：
低亮度青蓝 + 慢 pulse + 少量 mote + 低频氛围。
禁止高饱和 RPG loot glow。

## 37. Environmental Storytelling

全大陆至少30个环境叙事小场景，不是Quest：
- abandoned cart
- broken bridge
- old camp
- grave marker
- repaired fence
- washed-up cargo
- half-sunken boat
- collapsed quarry scaffold
- old logging site
- overgrown shrine
- city alley
- fisher basket
- ruined watch post
- picnic trace
- blocked mountain path
- burnt firepit
等。

至少10个没有任何物质奖励。
“发现本身”也是内容。

## 38. Micro-POI Density

Starter Village：≥12 distinct clusters
City：≥12
Port+Coast：≥10
每个主要 wilderness region：≥6

可以复用 prop，但布局不能复制粘贴感明显。

## 39. Decoration Library

至少建立：
- grass tuft x4+
- flowers x4+
- shrubs x3+
- mushrooms x3+
- fern
- fallen branch
- stump
- log
- pebble/rock cluster
- wood pile
- barrel
- crate
- sack
- fence variants
- sign
- bench
- lantern
- hay
- reed
- lily
- shell
- driftwood
- rope/net
- ancient fragment

Decor 能用 TileMap detail layer 就不要每个都变成 process Node。

## 40. MiniMap v3 —— P0

当前真人截图问题：
MiniMap 是右上角巨大黑色矩形。

必须彻底重做。

960×540 下：
- 建议160×96～180×108
- 不超过屏幕宽约20%
- margin 12～16px
- subtle pixel frame
- 半透明背景
- clip map content

MiniMap 显示：
**玩家周边局部窗口**，不是把整块大陆缩进去。

未探索区域：
透明/深雾，不用纯黑大面积覆盖。

已探索：
低饱和显示 land/forest/road/water/settlement/cliff。

Player：
高对比小 marker。

Landmark：
仅发现后显示。

Mine/Ruins：
优先隐藏 MiniMap，避免世界地图错误。

Settings 保留 MiniMap toggle。
可加入 `N` 快捷开关（无冲突前提）。

## 41. Full Map v3

`M`：
- discovered continent only
- coastline
- roads
- rivers
- settlements
- discovered landmarks
- player marker
- region names

未探索保持未知。
不显示 secrets/Echo/chests/enemies。

支持 pan / zoom / reset center。
视觉统一 Theme，不像 Debug 图。

## 42. HUD v3

Top Left：
- Day / Time
- Weather icon
- Season icon
- Hearts

减少文字，禁止类似“春 spring”重复语言。

Top Right：
MiniMap

Bottom Center：
6 slot Hotbar

Context：
只有靠近可交互物才显示短提示。

HUD 要轻，不能变任务面板。

## 43. UI Theme

创建项目统一 Theme：
`assets/ui/haven_theme.tres` 或等价。

统一：
- Button
- Panel
- Label
- Tooltip
- Slider
- Checkbox
- inventory slot
- map panel

风格：
dark warm charcoal + cream text + muted warm accent + 1/2px pixel border。

禁止默认 Godot Button 与自定义 UI 混用。

## 44. Inventory / Craft / Shop Polish

不改规则，只改呈现。

Inventory：
grid/icon/stack/selection/detail panel

Crafting：
可制作与缺材料清楚，不用巨大红色报错

Shop：
buy/sell/coins/detail，区域价格差不做“套利提示”。

## 45. Title Screen v3

第一张名片。

使用原创像素背景：
- 远山/第一大陆轮廓
- 前景草/树
- 海或风
- 少量云
- 极轻 parallax

`HAVEN`
New Game / Continue / Settings / Exit

不要开发信息堆满屏幕。

## 46. Pause / Settings

统一 Theme。

Settings 至少：
- Display
- Fullscreen
- Master
- Music
- Ambience
- SFX
- MiniMap
- Controls

若加入 screen shake：
提供 On/Off。

## 47. Gameplay Feedback Pass

对已有系统逐个补反馈：

Pickup：
小粒子 + soft sound + item rise/fade

Axe：
swing arc + wood chips + organic hit

Pickaxe：
rock chips + short impact + ore spark

Combat：
1～2px impact shake/flash + hurt flash + knockback

Dash：
short trail/ghost + subtle wind

Fishing：
bobber ripple + splash + bite feedback

Farming：
soil/water/crop stage清晰

Boat：
water wake/splash

Home Upgrade：
视觉状态真实改变

## 48. VFX Budget

优先 GPUParticles2D：
- rain
- fog mote
- firefly
- leaf
- dust
- wood chip
- stone chip
- splash
- Echo mote

控制同屏数量。
VFX 支持世界，不抢世界。

## 49. Audio v3 —— P0

真人明确反馈：
当前声音“像虫子一样滴滴滴滴，特别难听”。

必须找到实际触发来源。

禁止继续：
- repeated electronic beep
- square-wave drip
- 高频刺耳 tone
- 同一 sound 无变化反复播放

原则：
**坏声音不如暂时安静。**

## 50. Audio Bus

确认：
Master
├── Music
├── Ambience
├── SFX
└── UI

Master 加 limiter，避免 clipping。
声音必须路由到正确 bus。

Settings 分别控制 Master/Music/Ambience/SFX。

## 51. Ambient Layering

每区域声音分：
1. Background bed
2. Midground random ambience
3. Foreground positional audio

Foreground 使用 AudioStreamPlayer2D 及距离衰减。

区域最少：
Village：wind/birds/distant smith
Forest：leaves/birds/branches
Deep Forest：更少鸟、更深风/自然低频
Marsh：water/frog/natural insects/reeds
Farmland：wind/birds/distant animals
City：soft crowd/carts/workshop
Port：waves/gull/rope/wood creak
Coast：ocean/wind
Highlands：wind
Mine：cave air/drip
Ancient：very low drone/Echo resonance

区域切换 crossfade 2～5 秒。
禁止多个区域 ambience 一起叠满。

## 52. SFX 标准

高频动作至少3～5个 variation，轻微随机：
- sample
- pitch
- volume

Footstep 至少区分：
- grass
- dirt
- stone
- wood
- sand

短 SFX 建议约50～300ms，避免拖长。

## 53. 外部音频许可

允许用自然录音作为 Alpha 基础，但必须：
- CC0 / Public Domain / 明确允许商用；
- 来源可验证；
- 不从别的游戏提取；
- 不从 YouTube 随便下载；
- 不用不明“免费包”。

若使用外部音频：
新增 `THIRD_PARTY_NOTICES.md` 或 `data/audio_sources.json`，记录 file/source/license/author。

无法确认许可：不用。

没有合适资源：
宁可静音/低调程序化自然噪声，也不要电子 beep。

## 54. Loading 感知优化

当前 baked world 约2.4s。

不重建世界架构，但必须优化感知。

Title Scene 出现后立即：
`ResourceLoader.load_threaded_request("res://generated/first_continent_baked.tscn")`

跨帧使用：
`load_threaded_get_status`

禁止 while-loop 忙等。

准备好后才：
`load_threaded_get`

如果 New Game 时预加载已经完成：
fade → instantiate → play

如果未完成：
极简黑色过渡 + Haven 小符号。
不要 Tips/教程。

报告：
- cold load
- title-preloaded load
- perceived New Game wait

目标：
常见情况下 New Game 感知等待 <1秒。
达不到则如实报告。

## 55. Performance / TileMap

Static world：
Bake 后尽量不 runtime 大面积改 TileMap。

避免频繁 `update_internals()`。

Decor：
能进入 TileMap detail layer 的，不要全变 processing Node。

继续 chunk activation：
远处 NPC/enemy/resource/ambient foreground/particles 关闭或降频。

## 56. Save / Stable ID

禁止改变：
- stable object IDs
- region IDs
- world state keys
- save coordinate system

除非必须移动入口/建筑。
若旧 Save 坐标落入新障碍：
提供 safe-position recovery。

本轮默认继续 Save v4。
没有真实数据结构变更不要为了版本号好看升级 v5。

## 57. MiniMap / Fog Persistence

继续当前240×160 fog grid。
不要无意义提高分辨率让 save膨胀。

Save/Load 必须恢复 explored fog。
MiniMap 重做只改显示，不破坏数据。

## 58. Journal

继续保持“记录已知信息”，不是 Quest Tracker。

视觉可做 notebook-like panel。

分类：
- People
- Places
- Notes
- Echo Observations

不加 checkbox 主线任务。

## 59. 世界边界

大陆四周海洋自然成为陆地边界。

禁止沿海直接 invisible wall。

Boat 可航边界通过：
- deep sea
- current/horizon/fog
表达。

若硬边界必须存在：
放在玩家视觉上很远，且用海况自然提示。

## 60. Visual Regression Capture —— 必须新增

建立：
`tools/capture_v3_visuals.gd` 或等价。

自动捕获至少：
1. Title
2. Player Hut interior
3. Village day
4. Village night
5. Village rain
6. Central Valley
7. West Forest
8. Deep Forest
9. Marsh fog
10. Farmland
11. Quarry
12. City
13. Port night/sunset
14. South Coast
15. Lake day
16. Lake night
17. Highlands
18. Ancient Region
19. Mine interior
20. Ruins interior
21. Inventory
22. MiniMap
23. Full Map

输出：
`artifacts/visual_v3/`

禁止写用户文件夹。

## 61. Visual Capture 检查

Trae 最终必须检查每张截图：
- file exists
- correct dimensions
- not empty
- not black error
- no loading overlay left
- no obvious UI clipping

如果模型无法真正审美判断：
明确说明“视觉审美留给项目负责人”，禁止因为生成截图成功就声称画面好看。

## 62. 客观 Visual Gate

至少可自动验证：
- Player frame ≥22×30
- NPC visual combinations ≥8
- tree variants ≥7
- rock/ore variants ≥8
- core building variants ≥6
- ground variants ≥30
- terrain transition tiles exist
- water animation ≥8 frames
- project Theme exists
- MiniMap fits viewport
- MiniMap width ≤20% viewport
- unexplored MiniMap 不是全不透明纯黑
- Hotbar fits viewport
- Title fits 960×540
- Fog visual exists
- audio buses exist
- ugly legacy beep没有 runtime active reference
- 23 visual captures generated

## 63. 现有回归测试

14套 / 369项必须全部保留。
禁止删断言刷绿。

本轮可新增/扩展：
- visual_assets_test
- camera_pixel_test
- ui_theme_test
- minimap_visual_test
- lighting_weather_test
- audio_mix_test
- world_polish_test
- title_preload_test

重点是行为，不强制文件数。

## 64. Camera Test

验证：
- default=1.0
- wheel 0.5/1/2
- no continuous fractional tween
- HUD不动
- pixel alignment
- map edge behavior
- interior zoom不泄露大量外部空间

## 65. Building Regression

必须重新验证：
- New Game player visible
- hut roof first frame faded
- door passable
- enter/exit 3 times
- roof restores
- other small building fade
- furniture不堵门
- collision正常

不能因 art 重做再次出现“出不了房子”。

## 66. Audio Test

至少：
- buses存在
- UI→UI bus
- ambience→Ambience
- positional source适当使用AudioStreamPlayer2D
- legacy beep无runtime引用
- region ambience不叠满
- settings volume有效

## 67. Region Walkability

每个Region至少有一个 valid walkable sample。
主路不被decor/tree误堵。
Bridge/City gate/Port dock/Boat/Mine/Ruins入口必须可用。

## 68. Standalone 验证

最终必须用 **非 Embedded / Standalone game window** 验证：
- 1920×1080 fullscreen
- 1920×1080 windowed
- F11切换

最终报告必须区分：
- Editor Embedded Preview
- Standalone Game Window

不能把编辑器外黑色区域误判为游戏画面。

## 69. Playthrough

自动化 + 实际可走路线覆盖：

Title
→ New Game
→ Hut
→ walk out
→ Village
→ MiniMap
→ Forest
→ gathering/tool
→ return
→ shop
→ farmland
→ city
→ port
→ coast
→ boat
→ island
→ lake night
→ marsh
→ quarry/mine
→ highlands
→ ruins
→ save
→ title
→ continue

不得依赖 Debug Teleport 才能完成核心路线。

## 70. 第一印象 Gate

New Game 前60秒必须能看到：
- 人物真像人物
- Hut真像破茅草棚
- 室内有生活内容
- 村庄有细节
- NPC明显不同
- 道路不棋盘
- 草地不纯绿块
- MiniMap不是黑电视
- HUD不是Debug
- 没有刺耳滴滴声

任一明显失败：
本轮不能自评100%。

## 71. Region Identity Gate

隐藏HUD后只看截图，应能大致识别：
- Village
- Forest
- Marsh
- Farmland
- City
- Port
- Coast
- Highlands
- Ancient

若只是“换颜色的草地”：
失败。

## 72. Sound Identity Gate

闭眼听：
Village / Forest / Coast / Mine / Ancient
应明显不同。

不要求AAA，但禁止全世界同一个loop。

## 73. Performance Gate

目标：
- 60 FPS stable
- walk无明显周期卡顿
- MiniMap fog更新无明显 hitch
- 已预加载后 New Game 不再2.4s主线程冻结
- Map打开目标<200ms
- UI打开无卡顿
- static TileMap无持续重建

最终必须报告：
- FPS/采样方法
- cold load time
- preloaded load time
- map open time
- 最大明显 hitch

## 74. Error Gate

最终必须：
- 0 Parse Error
- 0 Runtime ERROR
- 0 error spam
- 0 recurring warning spam
- 0 missing resource
- 0 broken texture
- 0 UI clipping
- 0 spawn blocker
- 0 game-breaking collision
- 0 active ugly beep loop

## 75. Git

本轮有安全 checkpoint `316ebea`。

开发期间允许：
- git status
- git diff
- git diff --stat
- git log
- git show

禁止：
- git add
- git commit
- git push
- git reset
- git clean
- checkout -- .
- force push

本轮完成后等负责人真人试玩，再决定 checkpoint。

## 76. 删除规则

禁止删除：
- TASK.md
- CHARTER
- 用户真实Save
- 已稳定系统
- Git历史

旧 v2 visuals：
允许停止runtime引用，但不要大批删除。
先迁移→验证→报告unused list。

## 77. 外部视觉素材规则

允许网络做研究。
禁止：
- 复制其他游戏sprite
- Google图片直接进项目
- 水印素材
- 来源不明asset pack
- 版权不清资源

本轮核心视觉资产必须原创。
外部作品只参考比例、色彩、构图、密度、音频层次。

## 78. HAVEN_VISUAL_BIBLE.md 更新

完成后至少记录：
1. Visual Philosophy
2. Base Resolution
3. Pixel Snap
4. Camera Zoom
5. Tile Grid
6. Pixel Detail Unit
7. Master Palette
8. Region Palettes
9. Player Size
10. NPC Size
11. Character Silhouette
12. Tree Sizes
13. Rock Sizes
14. Building Footprints
15. Roof Language
16. Wall Language
17. Road Language
18. Water Language
19. Coast Language
20. Shadows
21. Lighting
22. Weather
23. Seasons
24. VFX
25. UI Theme
26. MiniMap
27. Full Map
28. Audio Identity
29. Region Ambience
30. Prohibited Patterns
31. Asset Generation Workflow
32. Screenshot Baselines

只写真实落地规则。

## 79. HAVEN_DESIGN.md / README

HAVEN_DESIGN.md 更新：
- v3 visual identity
- density
- environmental storytelling
- landmark navigation
- minimap
- lighting
- fog
- audio
- camera
- title preload
- known limitations

README 简洁更新：
- current version
- Godot version
- run
- controls
- current continent
- major features
- tests

不要把README写成施工书。

## 80. 推荐内部施工顺序

P0 Audit
→ P1 Pixel Pipeline / Palette / Camera
→ P2 Player & NPC
→ P3 Nature Assets
→ P4 Buildings & Interiors
→ P5 Region Dressing / POI / Landmarks
→ P6 UI / MiniMap / Full Map / Title
→ P7 Lighting / Weather / Seasons / VFX
→ P8 Audio
→ P9 Loading / Performance
→ P10 Full Regression
→ P11 Visual Capture
→ P12 Docs / SHA / Final Report

不要每个Phase停下来等确认。

## 81. Definition of Done

只有以下全部成立，才可宣称：
`HAVEN VISUAL & WORLD POLISH ALPHA v3.0 COMPLETE`

### First Impression
- Title像游戏
- New Game无Debug感
- Player可识别
- Hut是真正破旧茅草棚
- Village有生活密度

### Pixel Art
- 细像素
- 统一palette
- 人物/树/石/建筑比例统一
- 无巨大纯色对象

### World
- 13 Region有视觉身份
- 过渡自然
- ≥30环境叙事场景
- POI密度合格
- Landmark强化

### Water
- river/lake/ocean区分
- coast完整层级
- foam/wet sand
- port/coast有表现力

### Buildings
- 核心建筑不再纯几何
- interiors已装修
- roof fade正常

### UI
- 统一Theme
- compact HUD
- MiniMap不再黑框
- Full Map清楚
- Inventory/Craft/Shop统一

### Dynamic Visual
- day/night lighting
- clear/rain/fog
- seasons detail
- key lights
- restrained VFX

### Audio
- ugly beep移除
- buses正确
- layered ambience
- regional identity
- positional sounds
- core SFX合理

### Camera
- default 1.0
- 0.5/1/2 zoom
- crisp rendering
- HUD稳定

### Loading
- title threaded preload
- New Game不呈现“死机”
- 真实timing已报告

### Technical
- existing gameplay retained
- Save v4/migration intact
- stable IDs intact
- 369 old assertions retained
- all new tests pass
- 0 critical errors

### Visual Evidence
- ≥23 screenshots
- no black/error capture
- no clipping
- unresolved aesthetics explicitly listed

## 82. 最终汇报格式

A. Milestone
1. 实际完成比例
2. 未完成项
3. 不得虚报100%

B. Safety
4. starting commit
5. starting status
6. TASK SHA before/after
7. CHARTER SHA before/after

C. Pixel Pipeline
8. resolution
9. pixel snap
10. camera
11. tile grid
12. palette

D. Characters
13. player
14. animation
15. NPC variation
16. readability

E. Nature
17. ground
18. transitions
19. trees
20. rocks
21. water
22. coast
23. props

F. Buildings
24. hut
25. houses
26. shops
27. blacksmith
28. tavern
29. city
30. port
31. interiors
32. roof regression

G. World
33. 13 regions
34. POI count
35. environmental storytelling count
36. landmarks

H. UI/Map
37. HUD
38. Theme
39. MiniMap
40. Fog
41. Full Map
42. Inventory
43. Craft
44. Shop
45. Title

I. Lighting/VFX
46. day/night
47. rain
48. fog
49. seasons
50. lights
51. particles

J. Audio
52. ugly beep root cause
53. removed/replaced assets
54. bus layout
55. ambience
56. positional sound
57. variations
58. external licensed audio if any

K. Performance
59. cold load
60. preloaded load
61. FPS
62. map open
63. hitch findings

L. Tests
64. old regression count
65. new tests
66. total PASS/FAIL
67. standalone test
68. save/load
69. migration

M. Visual Captures
70. directory
71. screenshots
72. inspected screenshots
73. unresolved visual issues

N. Files
74. added
75. modified
76. legacy-unused candidates
77. git diff --stat
78. git status

O. Docs
79. HAVEN_DESIGN.md
80. HAVEN_VISUAL_BIBLE.md
81. README

P. Known Issues
82. current issues
83. next recommendation

完成后：
**停止开发，不 commit，不 push，等待项目负责人真人试玩。**
