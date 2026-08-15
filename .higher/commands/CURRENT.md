# Higher 可用命令清单

> 只记录标准命令。项目根：`C:\Users\37653\Desktop\Higher`（禁止在 Desktop 根目录执行 npm）。

## 标准 Gate（每次改动后）

```powershell
npx tsc --noEmit                                # 前端类型检查
cargo check --manifest-path src-tauri/Cargo.toml
cargo test  --manifest-path src-tauri/Cargo.toml
```

## 标准启动

```powershell
npm run tauri dev      # 桌面应用（含 Vite + Rust 编译 + Migration）
npm run dev            # 仅前端
npm run build          # 前端生产构建
```

## 环境注意

- cargo 默认不在 PATH：`$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"`
- Windows Smart App Control 可能拦截新编译测试 exe（os error 4551）：只确认 1 次，标记 ENV_BLOCKED，禁止重试/绕过（详见 ../ENVIRONMENT.md §29）

（历史临时命令不再保留；详史见 ../ai-operations/）
