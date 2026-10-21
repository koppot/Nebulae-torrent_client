#!/usr/bin/env python3
"""Static dump of a Blu-ray disc's HDMV navigation (read-only).

Prints index.bdmv (First Play / Top Menu / titles), every movie object
with its menu-call / title-search masks and decoded navigation commands,
and each playlist's duration, UO mask (menu call bit) and chapter count.

Usage: bdmv_nav_dump.py <disc root containing BDMV/>
"""
import os
import struct
import sys

GRP = {0: "BRANCH", 1: "CMP", 2: "SET"}
BR_GOTO = {0: "NOP", 1: "GOTO", 2: "BREAK"}
BR_JUMP = {0: "JumpObject", 1: "JumpTitle", 2: "CallObject", 3: "CallTitle", 4: "Resume"}
BR_PLAY = {0: "PlayPL", 1: "PlayPLatPI", 2: "PlayPLatMK", 3: "TerminatePL", 4: "LinkPI", 5: "LinkMK"}
CMP = {1: "BC", 2: "EQ", 3: "NE", 4: "GE", 5: "GT", 6: "LE", 7: "LT"}
SET = {1: "Move", 2: "Swap", 3: "Add", 4: "Sub", 5: "Mul", 6: "Div", 7: "Mod", 8: "Rnd",
       9: "And", 10: "Or", 11: "Xor", 12: "BitSet", 13: "BitClr", 14: "ShL", 15: "ShR"}
SETSYS = {1: "SetStream", 2: "SetNVTimer", 3: "SetButtonPage", 4: "EnableButton",
          5: "DisableButton", 6: "SetSecStream", 7: "PopupOff", 8: "StillOn", 9: "StillOff",
          10: "SetOutputMode", 11: "SetStreamSS", 16: "SetSystem0x10"}


def operand(val, imm):
    if imm:
        return str(val)
    if val & 0x80000000:
        return "PSR%d" % (val & 0x7F)
    return "GPR%d" % (val & 0xFFF)


def decode(cmd):
    w0, dst, src = struct.unpack(">III", cmd)
    op_cnt = (w0 >> 29) & 7
    grp = (w0 >> 27) & 3
    sub = (w0 >> 24) & 7
    imm1 = (w0 >> 23) & 1
    imm2 = (w0 >> 22) & 1
    br = (w0 >> 16) & 0xF
    cmpo = (w0 >> 8) & 0xF
    seto = w0 & 0x1F
    a = operand(dst, imm1)
    b = operand(src, imm2)
    if grp == 0:
        if sub == 0:
            name = BR_GOTO.get(br, "goto?%d" % br)
            return name + (" %s" % a if br == 1 else "")
        if sub == 1:
            name = BR_JUMP.get(br, "jump?%d" % br)
            return name + ("" if br == 4 else " %s" % a)
        if sub == 2:
            name = BR_PLAY.get(br, "play?%d" % br)
            if br in (1, 2):
                return "%s %s %s" % (name, a, b)
            if br == 3:
                return name
            return "%s %s" % (name, a)
        return "BRANCH?sub%d" % sub
    if grp == 1:
        return "if %s %s %s" % (a, CMP.get(cmpo, "cmp?%d" % cmpo), b if op_cnt > 1 else "")
    if grp == 2:
        if sub == 0:
            return "%s %s, %s" % (SET.get(seto, "set?%d" % seto), a, b)
        if sub == 1:
            return "%s dst=0x%08x src=0x%08x" % (SETSYS.get(seto, "sys?%d" % seto), dst, src)
    return "?? %08x %08x %08x" % (w0, dst, src)


def uo_bits(b8):
    v = int.from_bytes(b8, "big")
    names = ["menu_call", "title_search", "chapter_search", "time_search", "skip_next",
             "skip_prev", "play_firstplay", "stop", "pause_on", "pause_off", "still_off",
             "forward", "backward", "resume", "up", "down", "left", "right", "select",
             "activate", "select_activate", "audio_change", "-", "angle", "popup_on", "popup_off"]
    on = [n for i, n in enumerate(names) if n != "-" and (v >> (63 - i)) & 1]
    return on


def parse_index(path):
    d = open(path, "rb").read()
    assert d[:4] == b"INDX", d[:8]
    idx_start = struct.unpack(">I", d[8:12])[0]
    p = idx_start + 4

    def obj(p, title=False):
        w = struct.unpack(">I", d[p:p + 4])[0]
        otype = w >> 30
        access = (w >> 28) & 3
        p += 4
        if otype == 1:
            w2 = struct.unpack(">I", d[p:p + 4])[0]
            ptype = w2 >> 30
            ref = w2 & 0xFFFF
            desc = "HDMV %s MObj%d" % (["movie", "interactive", "?2", "?3"][ptype], ref)
        elif otype == 2:
            w2 = struct.unpack(">I", d[p:p + 4])[0]
            ptype = w2 >> 30
            name = d[p + 4:p + 9].decode("ascii", "replace")
            desc = "BD-J %s %s.bdjo" % (["?0", "?1", "movie", "interactive"][ptype], name)
        else:
            desc = "none(type %d)" % otype
        if title:
            desc += " access=%d" % access
        return desc, p + 8

    fp, p = obj(p)
    tm, p = obj(p)
    n = struct.unpack(">H", d[p:p + 2])[0]
    p += 2
    titles = []
    for _ in range(n):
        t, p = obj(p, True)
        titles.append(t)
    return fp, tm, titles


def parse_mobj(path):
    d = open(path, "rb").read()
    assert d[:4] == b"MOBJ", d[:8]
    p = 48
    n = struct.unpack(">H", d[p:p + 2])[0]
    p += 2
    objs = []
    for _ in range(n):
        flags = struct.unpack(">H", d[p:p + 2])[0]
        ncmd = struct.unpack(">H", d[p + 2:p + 4])[0]
        p += 4
        cmds = [decode(d[p + 12 * i:p + 12 * i + 12]) for i in range(ncmd)]
        p += 12 * ncmd
        objs.append({
            "resume": (flags >> 15) & 1,
            "menu_call_mask": (flags >> 14) & 1,
            "title_search_mask": (flags >> 13) & 1,
            "cmds": cmds,
        })
    return objs


def parse_mpls(path):
    d = open(path, "rb").read()
    pl_start, mk_start = struct.unpack(">II", d[8:16])
    ai_uo = uo_bits(d[48:56])
    p = pl_start + 4 + 2
    nitems = struct.unpack(">H", d[p:p + 2])[0]
    p += 4
    total = 0
    items = []
    for _ in range(nitems):
        ln = struct.unpack(">H", d[p:p + 2])[0]
        clip = d[p + 2:p + 7].decode("ascii", "replace")
        tin, tout = struct.unpack(">II", d[p + 14:p + 22])
        pi_uo = uo_bits(d[p + 22:p + 30])
        total += tout - tin
        items.append((clip, (tout - tin) / 45000.0, pi_uo))
        p += 2 + ln
    nm = struct.unpack(">H", d[mk_start + 4:mk_start + 6])[0]
    entry = 0
    for i in range(nm):
        m = mk_start + 6 + 14 * i
        if d[m + 1] == 1:
            entry += 1
    return total / 45000.0, ai_uo, items, entry


def main():
    root = sys.argv[1]
    bdmv = os.path.join(root, "BDMV")
    fp, tm, titles = parse_index(os.path.join(bdmv, "index.bdmv"))
    print("index.bdmv")
    print("  FirstPlay: %s" % fp)
    print("  TopMenu:   %s" % tm)
    for i, t in enumerate(titles, 1):
        print("  Title %d:   %s" % (i, t))
    mobj_path = os.path.join(bdmv, "MovieObject.bdmv")
    if os.path.exists(mobj_path):
        objs = parse_mobj(mobj_path)
        limit = int(os.environ.get("MOBJ_CMD_LIMIT", "80"))
        for i, o in enumerate(objs):
            print("MObj%d resume=%d menu_call_mask=%d title_search_mask=%d cmds=%d" % (
                i, o["resume"], o["menu_call_mask"], o["title_search_mask"], len(o["cmds"])))
            for j, c in enumerate(o["cmds"][:limit]):
                print("    %4d  %s" % (j, c))
            if len(o["cmds"]) > limit:
                print("    ... (%d more)" % (len(o["cmds"]) - limit))
    pl_dir = os.path.join(bdmv, "PLAYLIST")
    for name in sorted(os.listdir(pl_dir)):
        if not name.lower().endswith(".mpls"):
            continue
        try:
            dur, ai_uo, items, chapters = parse_mpls(os.path.join(pl_dir, name))
        except Exception as e:  # noqa: BLE001
            print("%s: parse error %s" % (name, e))
            continue
        masked = "menu_call MASKED" if "menu_call" in ai_uo else "menu_call allowed"
        first_pi = items[0] if items else None
        pi_mc = ("first-PI menu_call MASKED" if first_pi and "menu_call" in first_pi[2] else "")
        print("%s: %8.1fs items=%d chapters=%d [%s] %s clips=%s" % (
            name, dur, len(items), chapters, masked, pi_mc,
            ",".join("%s(%.0fs)" % (c, s) for c, s, _ in items[:6]) + (",..." if len(items) > 6 else "")))


if __name__ == "__main__":
    main()
