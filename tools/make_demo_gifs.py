#!/usr/bin/env python3
"""Generate animated terminal GIFs for the mdout README.

Renders real `mdout` output (captured with --color=always) into a fake
terminal window using Pillow, then assembles the frames into GIFs.

Usage: python3 tools/make_demo_gifs.py [BIN]
"""

import os
import re
import subprocess
import sys

from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "mdout")
OUT_DIR = os.path.join(ROOT, "docs", "assets")

FONT_PATH = "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc"
FONT_INDEX = 1
FONT_SIZE = 20

COLS = 74
ROWS = 24
CELL_W = 12
CELL_H = 27
PAD = 20
TITLE_H = 36

BG = (13, 17, 23)
PANEL = (13, 17, 23)
BORDER = (48, 54, 61)
TITLE_BG = (22, 27, 34)
FG = (201, 209, 217)
DIM = (110, 118, 129)
CYAN = (88, 166, 255)
PROMPT = (126, 231, 135)
CURSOR = (139, 148, 158)
RED = (255, 95, 86)
YELLOW = (255, 189, 46)
GREEN = (39, 201, 63)

WIDTH = COLS * CELL_W + 2 * PAD
HEIGHT = TITLE_H + PAD + ROWS * CELL_H + PAD

FONT = ImageFont.truetype(FONT_PATH, FONT_SIZE, index=FONT_INDEX)
ASCENT, DESCENT = FONT.getmetrics()

SGR_RE = re.compile(r"\x1b\[([0-9;]*)m")


def default_style():
    return {"fg": None, "bold": False, "dim": False, "reverse": False, "strike": False}


def parse_ansi(text):
    lines = []
    cells = []
    style = default_style()
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "\x1b":
            m = SGR_RE.match(text, i)
            if m:
                apply_sgr(style, m.group(1))
                i = m.end()
                continue
            i += 1
            continue
        if ch == "\n":
            lines.append(cells)
            cells = []
            i += 1
            continue
        cells.append((ch, dict(style)))
        i += 1
    lines.append(cells)
    return lines


def apply_sgr(style, params):
    if params == "":
        parts = [0]
    else:
        parts = [int(p) if p else 0 for p in params.split(";")]
    i = 0
    while i < len(parts):
        p = parts[i]
        if p == 0:
            style.update(default_style())
        elif p == 1:
            style["bold"] = True
        elif p == 2:
            style["dim"] = True
        elif p == 3:
            pass
        elif p == 7:
            style["reverse"] = True
        elif p == 9:
            style["strike"] = True
        elif p == 36:
            style["fg"] = CYAN
        elif p == 39:
            style["fg"] = None
        elif p == 38 and i + 4 < len(parts) and parts[i + 1] == 2:
            style["fg"] = (parts[i + 2], parts[i + 3], parts[i + 4])
            i += 4
        elif 30 <= p <= 37:
            basic = [(0, 0, 0), (255, 95, 86), (39, 201, 63), (255, 189, 46),
                     (88, 166, 255), (188, 140, 255), (57, 197, 187), (201, 209, 217)]
            style["fg"] = basic[p - 30]
        i += 1


def char_cols(ch):
    import unicodedata

    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def cell_color(style):
    fg = style["fg"] or FG
    bg = BG
    if style["dim"]:
        fg = DIM if style["fg"] is None else tuple(int(c * 0.65) for c in fg)
    if style["reverse"]:
        fg, bg = bg, fg
    return fg, bg


def draw_line(draw, y, cells, cursor=False):
    x = PAD
    baseline = y + ASCENT
    for ch, style in cells:
        w = char_cols(ch)
        fg, bg = cell_color(style)
        if bg != BG:
            draw.rectangle([x, y, x + w * CELL_W - 1, y + CELL_H - 1], fill=bg)
        stroke = 1 if style["bold"] else 0
        draw.text((x, baseline), ch, font=FONT, fill=fg, anchor="ls",
                  stroke_width=stroke, stroke_fill=fg)
        if style["strike"]:
            mid = y + ASCENT - FONT_SIZE // 3
            draw.line([x, mid, x + w * CELL_W - 1, mid], fill=fg, width=2)
        x += w * CELL_W
    if cursor:
        draw.rectangle([x, y + 2, x + CELL_W - 1, y + CELL_H - 3], fill=CURSOR)
    return x


def render_frame(cells_lines, cursor=False):
    img = Image.new("RGB", (WIDTH, HEIGHT), (10, 12, 16))
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle([0, 0, WIDTH - 1, HEIGHT - 1], radius=12,
                           fill=PANEL, outline=BORDER, width=1)
    draw.rounded_rectangle([0, 0, WIDTH - 1, TITLE_H], radius=12, fill=TITLE_BG)
    draw.rectangle([0, TITLE_H - 12, WIDTH - 1, TITLE_H], fill=TITLE_BG)
    for dx, color in ((18, RED), (38, YELLOW), (58, GREEN)):
        draw.ellipse([dx, TITLE_H // 2 - 6, dx + 12, TITLE_H // 2 + 6], fill=color)
    title = "mdout  ·  live markdown in your terminal"
    tw = draw.textlength(title, font=FONT)
    draw.text(((WIDTH - tw) / 2, TITLE_H // 2), title, font=FONT, fill=DIM, anchor="lm")
    draw.line([0, TITLE_H, WIDTH - 1, TITLE_H], fill=BORDER, width=1)

    view = cells_lines[-ROWS:]
    y = TITLE_H + PAD
    for idx, line in enumerate(view):
        last = cursor and idx == len(view) - 1
        draw_line(draw, y, line, cursor=last)
        y += CELL_H
    return img


def prompt_line(cmd):
    cells = [("$", {"fg": PROMPT, **{k: v for k, v in default_style().items() if k != "fg"}}),
             (" ", default_style())]
    for ch in cmd:
        cells.append((ch, default_style()))
    return cells


def mdout_render(md):
    proc = subprocess.run(
        [BIN, "--color=always", f"--width={COLS}"],
        input=md.encode("utf-8"),
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return parse_ansi(proc.stdout.decode("utf-8", "replace"))


def build_stream_gif(path):
    cmd = "tail -f answer.md | mdout"
    md = (
        "# TCP 三次握手\n"
        "\n"
        "TCP 通过三次握手建立可靠连接：\n"
        "\n"
        "1. **SYN**：客户端发送 `SYN=1, seq=x`\n"
        "2. **SYN+ACK**：服务端回复 `SYN=1, ACK=1`\n"
        "3. **ACK**：客户端确认，连接建立\n"
        "\n"
        "| 步骤 | 报文 | 方向 |\n"
        "| :--: | :-- | :-- |\n"
        "| 1 | SYN | C → S |\n"
        "| 2 | SYN+ACK | S → C |\n"
        "| 3 | ACK | C → S |\n"
        "\n"
        "```rust\n"
        "fn handshake() -> bool {\n"
        "    true\n"
        "}\n"
        "```\n"
    )
    frames, durations = [], []

    for k in range(1, len(cmd) + 1):
        frames.append(render_frame([prompt_line(cmd[:k])], cursor=True))
        durations.append(55)

    total = len(md)
    steps = 46
    for s in range(1, steps + 1):
        cut = max(1, round(total * s / steps))
        prefix = md[:cut]
        body = [prompt_line(cmd)] + mdout_render(prefix)
        frames.append(render_frame(body, cursor=s < steps))
        durations.append(110)

    final = render_frame([prompt_line(cmd)] + mdout_render(md), cursor=False)
    frames.append(final)
    durations.append(1800)

    save_gif(frames, durations, path)


def build_features_gif(path):
    cmd = "cat features.md | mdout"
    md = (
        "## mdout 能做什么\n"
        "\n"
        "> 把管道里的 Markdown **实时** 渲染成终端文档。\n"
        "\n"
        "- 粗体 **strong**、斜体 *emphasis*、`行内代码`、~~删除线~~\n"
        "- 中文与 English 混排，表格也能对齐\n"
        "- 嵌套列表：\n"
        "  - 支持两级缩进与悬挂对齐\n"
        "  - 引用、代码、表格都好看\n"
        "\n"
        "```python\n"
        "def greet(name):\n"
        "    return f\"hello, {name}\"\n"
        "```\n"
        "\n"
        "---\n"
        "\n"
        "链接：[mdout](https://github.com/sixiaozhe/MDout)\n"
    )
    frames, durations = [], []
    for k in range(1, len(cmd) + 1):
        frames.append(render_frame([prompt_line(cmd[:k])], cursor=True))
        durations.append(45)
    frames.append(render_frame([prompt_line(cmd)] + mdout_render(md), cursor=False))
    durations.append(2200)
    save_gif(frames, durations, path)


def build_table_gif(path):
    cmd = "cat report.md | mdout"
    md = (
        "| 名称 | 描述 | 数值 |\n"
        "| :-- | :-- | --: |\n"
        "| 中文 | 简中与英文混排 mixed | 1 |\n"
        "| 名称 | CJK alignment | 42 |\n"
        "| metric | 宽度按显示列计算 | 1024 |\n"
    )
    frames, durations = [], []
    for k in range(1, len(cmd) + 1):
        frames.append(render_frame([prompt_line(cmd[:k])], cursor=True))
        durations.append(60)
    frames.append(render_frame([prompt_line(cmd)] + mdout_render(md), cursor=False))
    durations.append(2600)
    save_gif(frames, durations, path)


def save_gif(frames, durations, path):
    pal = [f.convert("P", palette=Image.ADAPTIVE, colors=64) for f in frames]
    pal[0].save(path, save_all=True, append_images=pal[1:], duration=durations,
                loop=0, optimize=True, disposal=2)
    print("%s  %d frames  %.1f KB" % (path, len(frames), os.path.getsize(path) / 1024))


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    build_stream_gif(os.path.join(OUT_DIR, "stream.gif"))
    build_features_gif(os.path.join(OUT_DIR, "features.gif"))
    build_table_gif(os.path.join(OUT_DIR, "table.gif"))


if __name__ == "__main__":
    main()
