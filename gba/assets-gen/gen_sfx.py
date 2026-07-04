#!/usr/bin/env python3
"""retris の効果音・BGM を決定的に合成して gba/sfx/*.wav に出力する。

- 依存: Python3 標準ライブラリのみ (wave / math / struct)。
- フォーマット: モノラル 16bit PCM, 10512 Hz。
  agb の mixer を Frequency::Hz10512 で初期化するため、サンプルレートは
  これと完全一致していなければならない (agb はリサンプリングしない)。
- 乱数 (ノイズ) は固定シードの LCG なので、出力はバイト単位で冪等。

使い方: python3 gba/assets-gen/gen_sfx.py
"""

import math
import struct
import wave
from pathlib import Path

RATE = 10512  # agb::sound::mixer::Frequency::Hz10512 と一致させること
OUT_DIR = Path(__file__).resolve().parent.parent / "sfx"

# ---------------------------------------------------------------------------
# 波形合成の基本部品
# ---------------------------------------------------------------------------

_lcg_state = 0x1234_5678


def _lcg() -> float:
    """固定シード LCG による -1.0..1.0 の擬似乱数 (決定的ノイズ源)。"""
    global _lcg_state
    _lcg_state = (_lcg_state * 1664525 + 1013904223) & 0xFFFF_FFFF
    return (_lcg_state >> 16) / 32768.0 - 1.0


def square(dur, f0, f1=None, vol=0.4, duty=0.5, decay=0.0):
    """矩形波を生成する。

    f1 を与えると f0 → f1 へ指数的にピッチスイープする。
    decay は音量の指数減衰係数 (1/秒)。クリックノイズ防止に
    先頭 1.5ms のアタックと末尾 3ms のリリースを掛ける。
    """
    if f1 is None:
        f1 = f0
    n = int(dur * RATE)
    attack = max(1, int(0.0015 * RATE))
    release = max(1, int(0.003 * RATE))
    out = []
    phase = 0.0
    for i in range(n):
        t = i / n
        freq = f0 * (f1 / f0) ** t
        phase = (phase + freq / RATE) % 1.0
        env = math.exp(-decay * i / RATE)
        env *= min(1.0, i / attack, (n - 1 - i) / release)
        out.append((vol if phase < duty else -vol) * env)
    return out


def noise(dur, vol=0.4, decay=0.0, hold=1):
    """ノイズを生成する。hold で同じ乱数値を保持して音色を低域寄りにする。"""
    n = int(dur * RATE)
    release = max(1, int(0.003 * RATE))
    out = []
    cur = 0.0
    for i in range(n):
        if i % hold == 0:
            cur = _lcg()
        env = math.exp(-decay * i / RATE)
        env *= min(1.0, (n - 1 - i) / release)
        out.append(cur * vol * env)
    return out


def silence(dur):
    return [0.0] * int(dur * RATE)


def cat(*parts):
    """波形を順に連結する。"""
    out = []
    for p in parts:
        out.extend(p)
    return out


def mix(*tracks):
    """波形を重ね合わせる (長さは最長に合わせる)。"""
    n = max(len(t) for t in tracks)
    out = [0.0] * n
    for t in tracks:
        for i, s in enumerate(t):
            out[i] += s
    return out


def write_wav(name, samples):
    path = OUT_DIR / name
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)  # 16bit (agb 側で上位 8bit に縮約される)
        w.setframerate(RATE)
        frames = bytearray()
        for s in samples:
            v = max(-1.0, min(1.0, s))
            frames += struct.pack("<h", int(v * 32767))
        w.writeframes(bytes(frames))
    print(f"  {name}: {len(samples) / RATE:.3f}s ({len(samples)} samples)")


# ---------------------------------------------------------------------------
# 音名 → 周波数
# ---------------------------------------------------------------------------

_NOTE_INDEX = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}


def note(name: str) -> float:
    """"E5" / "A#4" / "Bb3" 形式の音名を周波数 (Hz) に変換する。"""
    letter = name[0]
    rest = name[1:]
    semitone = _NOTE_INDEX[letter]
    if rest.startswith("#"):
        semitone += 1
        rest = rest[1:]
    elif rest.startswith("b"):
        semitone -= 1
        rest = rest[1:]
    octave = int(rest)
    midi = (octave + 1) * 12 + semitone
    return 440.0 * 2.0 ** ((midi - 69) / 12.0)


def arpeggio(names, note_dur, last_dur=None, vol=0.4, duty=0.5, decay=6.0):
    """音名列を順に鳴らすアルペジオ。最後の音だけ長さを変えられる。"""
    parts = []
    for i, n in enumerate(names):
        dur = last_dur if (last_dur is not None and i == len(names) - 1) else note_dur
        parts.append(square(dur, note(n), vol=vol, duty=duty, decay=decay))
    return cat(*parts)


# ---------------------------------------------------------------------------
# 効果音
# ---------------------------------------------------------------------------


def gen_sfx():
    # 移動: ごく短いブリップ。
    write_wav("move.wav", square(0.030, 950, vol=0.40, decay=50.0))

    # 回転: 上昇ブリップ。
    write_wav("rotate.wav", square(0.050, 550, 1100, vol=0.40, decay=25.0))

    # ホールド: 2 音のスワップ感 (下→上)。
    write_wav(
        "hold.wav",
        cat(
            square(0.045, 700, vol=0.40, decay=15.0),
            square(0.060, 1050, vol=0.40, decay=20.0),
        ),
    )

    # ハードドロップ: 低域スイープ + ノイズで「ドスン」。
    write_wav(
        "hard_drop.wav",
        mix(
            square(0.090, 160, 55, vol=0.55, decay=22.0),
            noise(0.050, vol=0.35, decay=55.0, hold=2),
        ),
    )

    # 設置: 短いクリック。
    write_wav(
        "lock.wav",
        mix(
            square(0.030, 240, 180, vol=0.40, decay=60.0),
            noise(0.018, vol=0.22, decay=90.0),
        ),
    )

    # ライン消去: 消去数で音数と長さが増える (E5 → A5 → C6 → …)。
    write_wav("line_clear_1.wav", arpeggio(["E5", "A5"], 0.055, 0.100, decay=8.0))
    write_wav(
        "line_clear_2.wav", arpeggio(["E5", "A5", "C6"], 0.055, 0.110, decay=8.0)
    )
    write_wav(
        "line_clear_3.wav",
        arpeggio(["E5", "A5", "C6", "E6"], 0.055, 0.120, decay=8.0),
    )

    # テトリス (4 列): 派手なファンファーレ的アルペジオ。
    write_wav(
        "tetris.wav",
        cat(
            arpeggio(["C5", "E5", "G5", "C6"], 0.050, decay=4.0),
            arpeggio(["G5", "C6", "E6"], 0.050, decay=4.0),
            square(0.250, note("C6"), vol=0.42, decay=7.0),
        ),
    )

    # T-Spin: 2 音を高速交互に鳴らすトリル (特徴的な音)。
    write_wav(
        "tspin.wav",
        cat(*(square(0.032, note(n), vol=0.40, decay=4.0) for n in ["F#5", "B5"] * 4)),
    )

    # パーフェクトクリア: キラキラ上昇アルペジオ (2 オクターブ)。
    write_wav(
        "perfect_clear.wav",
        cat(
            arpeggio(
                ["C5", "E5", "G5", "C6", "E6", "G6"], 0.058, vol=0.38, duty=0.25, decay=4.0
            ),
            square(0.200, note("C7"), vol=0.38, duty=0.25, decay=8.0),
        ),
    )

    # レベルアップ: 上昇 3 音。
    write_wav("level_up.wav", arpeggio(["C5", "G5", "C6"], 0.070, 0.130, decay=5.0))

    # ゲームオーバー: 下降音列 + 低域グライド。
    write_wav(
        "game_over.wav",
        cat(
            square(0.140, note("E5"), vol=0.40, decay=3.0),
            square(0.140, note("C5"), vol=0.40, decay=3.0),
            square(0.140, note("A4"), vol=0.40, decay=3.0),
            square(0.380, note("E4"), note("E3"), vol=0.42, decay=4.0),
        ),
    )

    # ゲーム開始 (タイトルで START): 明るい上昇アルペジオ。
    write_wav(
        "start.wav",
        cat(
            arpeggio(["G4", "C5", "E5", "G5"], 0.055, decay=4.0),
            square(0.160, note("C6"), vol=0.42, decay=7.0),
        ),
    )


# ---------------------------------------------------------------------------
# BGM: コロブチカ (テトリスのテーマ A)
#
# 19 世紀ロシア民謡でパブリックドメイン。テーマ A の主旋律 16 小節 (2/4 拍子)
# を矩形波 2 声 (メロディ + ベース) で 1 ループ分合成する。
# ---------------------------------------------------------------------------

BPM = 149.0
BEAT = 60.0 / BPM  # 4 分音符の長さ (秒)

# メロディ: (音名 | None=休符, 拍数) のリスト。2/4 なので 1 小節 = 2 拍。
MELODY = [
    # 前半 8 小節
    ("E5", 1.0), ("B4", 0.5), ("C5", 0.5),
    ("D5", 1.0), ("C5", 0.5), ("B4", 0.5),
    ("A4", 1.0), ("A4", 0.5), ("C5", 0.5),
    ("E5", 1.0), ("D5", 0.5), ("C5", 0.5),
    ("B4", 1.5), ("C5", 0.5),
    ("D5", 1.0), ("E5", 1.0),
    ("C5", 1.0), ("A4", 1.0),
    ("A4", 2.0),
    # 後半 8 小節
    ("D5", 1.5), ("F5", 0.5),
    ("A5", 1.0), ("G5", 0.5), ("F5", 0.5),
    ("E5", 1.5), ("C5", 0.5),
    ("E5", 1.0), ("D5", 0.5), ("C5", 0.5),
    ("B4", 1.0), ("B4", 0.5), ("C5", 0.5),
    ("D5", 1.0), ("E5", 1.0),
    ("C5", 1.0), ("A4", 1.0),
    ("A4", 2.0),
]

# ベース: 小節ごとのコードルート。各小節をルートの 8 分音符 4 つ
# (低オクターブ・高オクターブ交互) で刻む。
BASS_ROOTS = [
    # 前半 8 小節: Em B7 Am Em B7 Em Am Am
    "E2", "B2", "A2", "E2", "B2", "E2", "A2", "A2",
    # 後半 8 小節: Dm Dm C C B7 Em Am Am
    "D2", "D2", "C2", "C2", "B2", "E2", "A2", "A2",
]


def _render_track(events, total_samples, vol, duty, decay, gate=0.92):
    """(開始拍, 長さ拍, 周波数 or None) の列を 1 本の波形に描画する。

    開始位置は累積拍から都度計算するため、音符間で誤差が蓄積しない。
    gate で音符の後端を少し切って発音を区切る。
    """
    buf = [0.0] * total_samples
    for start_beat, dur_beat, freq in events:
        if freq is None:
            continue
        start = int(start_beat * BEAT * RATE)
        tone = square(dur_beat * BEAT * gate, freq, vol=vol, duty=duty, decay=decay)
        for i, s in enumerate(tone):
            if start + i < total_samples:
                buf[start + i] += s
    return buf


def gen_bgm():
    total_beats = sum(d for _, d in MELODY)
    total_samples = int(total_beats * BEAT * RATE)

    melody_events = []
    beat = 0.0
    for name, dur in MELODY:
        melody_events.append((beat, dur, note(name) if name else None))
        beat += dur

    bass_events = []
    for m, root in enumerate(BASS_ROOTS):
        base = note(root)
        for k in range(4):  # 8 分音符 4 つ (ルート/オクターブ上の交互)
            freq = base if k % 2 == 0 else base * 2.0
            bass_events.append((m * 2.0 + k * 0.5, 0.5, freq))

    melody = _render_track(melody_events, total_samples, vol=0.26, duty=0.5, decay=1.2)
    bass = _render_track(bass_events, total_samples, vol=0.20, duty=0.25, decay=3.0)
    write_wav("bgm_korobeiniki.wav", mix(melody, bass))


def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    print(f"writing to {OUT_DIR} ({RATE} Hz, mono, 16bit)")
    gen_sfx()
    gen_bgm()


if __name__ == "__main__":
    main()
