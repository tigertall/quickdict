# QuickDict

GNOME desktop dictionary and text selection translation tool, the advantage is that it can achieve text selection translation and in-place display under Wayland when used with gnome-shell extensions.

GNOME 桌面的词典与划词翻译工具，优势是搭配 gnome-shell 扩展在wayland下可以实现划词翻译和原地显示。

<div style="display: flex; justify-content: space-between;">
<img src="https://raw.githubusercontent.com/tigertall/quickdict/main/screenshots/word-capture.png" alt="word-capture" width="40%">
<img src="https://raw.githubusercontent.com/tigertall/quickdict/main/screenshots/main-window.png" alt="main-window" width="40%">
</div>

## 特色

- **划词翻译** — GNOME Shell 扩展实现选中即译，Wayland 原生支持，无需额外操作
- **应用过滤** — 扩展支持设置划词的白名单应用，避免干扰其他应用进程
- **离线词典** — 支持 StarDict 和 MDX 格式，查询不依赖网络
- **在线翻译** — 划词优先离线、离线未命中才请求在线，避免 API 消耗；主界面搜索离线在线并行，展示全部来源
- **模糊搜索** — 基于 Levenshtein 距离的容错匹配，拼写偏差也能找到
- **词典排序** — 自定义词典查询顺序

## 构建与部署

词典主程序可选 Flatpak构建或者本地构建；gnome-shell 插件本地构建安装。

### Flatpak 构建

```bash
# 安装 Flatpak 与 SDK
sudo dnf install flatpak-builder
flatpak install --user -y flathub \
  org.freedesktop.Platform//25.08 \
  org.freedesktop.Sdk//25.08 \
  org.gnome.Platform//50 org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08

# 构建并安装
flatpak-builder --user --force-clean --install --ccache \
  --default-branch=stable build-dir \
  io.github.tigertall.QuickDict.json
```

### Meson 构建（系统安装）

如果不想要flatpak安装包，可以考虑这种构建方式。

环境要求：GNOME 50、Rust

```bash
# 构建并安装
meson setup builddir --prefix=/usr --buildtype=release
sudo meson install -C builddir
```

### GNOME Shell 扩展（用户级，无需 sudo）

```bash
# 构建扩展 zip 并安装
meson setup _ext_build ext/
meson compile -C _ext_build
gnome-extensions install --force _ext_build/quickdict-focus@tigertall.github.com.zip
```

> 注销后重新登录，扩展生效。

## 词典格式

- **StarDict**（.ifo/.idx/.dict 或 .ifo/.idx/.dict.dz）
- **MDX**（.mdx/.mdd）

在首选项 → Dictionaries 中添加词典目录即可。

## 许可证

MIT
