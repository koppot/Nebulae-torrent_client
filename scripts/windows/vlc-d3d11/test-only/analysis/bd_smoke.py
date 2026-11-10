"""Blu-ray smoke test of a staged Nebulae VLC runtime (test-only, read-only on the disc folder).

usage: python bd_smoke.py RESOURCES_DIR BD_FOLDER menu|feature OUT_DIR

RESOURCES_DIR is the `resources` directory of the extracted installer payload (holds vlc\\ and jre\\).
Runs in an interactive desktop session; libvlc opens its own Direct3D 11 window.
"""
import ctypes, os, sys, time, urllib.parse

res, disc, mode, out = sys.argv[1:5]
os.makedirs(out, exist_ok=True)
vlc_dir = os.path.join(res, "vlc")
jre = os.path.join(res, "jre")
os.environ["JAVA_HOME"] = jre
os.environ["PATH"] = os.path.join(jre, "bin") + os.pathsep + vlc_dir + os.pathsep + os.environ["PATH"]
os.environ["VLC_PLUGIN_PATH"] = os.path.join(vlc_dir, "plugins")
os.add_dll_directory(vlc_dir)
os.add_dll_directory(os.path.join(jre, "bin"))
v = ctypes.CDLL(os.path.join(vlc_dir, "libvlc.dll"))

P, I, LL, S = ctypes.c_void_p, ctypes.c_int, ctypes.c_longlong, ctypes.c_char_p
def fn(name, res_t, *arg_t):
    f = getattr(v, name, None)
    if f:
        f.restype, f.argtypes = res_t, list(arg_t)
    return f

new = fn("libvlc_new", P, I, ctypes.POINTER(S))
loc = fn("libvlc_media_new_location", P, P, S)
mp_new = fn("libvlc_media_player_new_from_media", P, P)
play = fn("libvlc_media_player_play", I, P)
stop = fn("libvlc_media_player_stop", None, P)
state = fn("libvlc_media_player_get_state", I, P)
gtime = fn("libvlc_media_player_get_time", LL, P)
stime = fn("libvlc_media_player_set_time", None, P, LL)
glen = fn("libvlc_media_player_get_length", LL, P)
gtitle = fn("libvlc_media_player_get_title", I, P)
ntitle = fn("libvlc_media_player_get_title_count", I, P)
nchap = fn("libvlc_media_player_get_chapter_count", I, P)
gchap = fn("libvlc_media_player_get_chapter", I, P)
schap = fn("libvlc_media_player_set_chapter", None, P, I)
pause = fn("libvlc_media_player_set_pause", None, P, I)
naudio = fn("libvlc_audio_get_track_count", I, P)
saudio = fn("libvlc_audio_set_track", I, P, I)
gaudio = fn("libvlc_audio_get_track", I, P)
nspu = fn("libvlc_video_get_spu_count", I, P)
sspu = fn("libvlc_video_set_spu", I, P, I)
snap = fn("libvlc_video_take_snapshot", I, P, ctypes.c_uint, S, ctypes.c_uint, ctypes.c_uint)
class TrackDesc(ctypes.Structure):
    pass
TrackDesc._fields_ = [("i_id", I), ("psz_name", S), ("p_next", ctypes.POINTER(TrackDesc))]
audio_desc = fn("libvlc_audio_get_track_description", ctypes.POINTER(TrackDesc), P)
spu_desc = fn("libvlc_video_get_spu_description", ctypes.POINTER(TrackDesc), P)

def ids(lst):
    out, n = [], lst
    while n:
        out.append((n.contents.i_id, (n.contents.psz_name or b"").decode("utf-8", "replace")))
        n = n.contents.p_next
    return out

pl_id = fn("libvlc_media_player_get_bluray_playlist_id", I, P)
menu_open = fn("libvlc_media_player_get_bluray_menu_open", I, P)
nav_menu = fn("libvlc_media_player_navigate_menu", None, P)
mp_rel = fn("libvlc_media_player_release", None, P)
rel = fn("libvlc_release", None, P)
sz = fn("libvlc_video_get_size", I, P, ctypes.c_uint, ctypes.POINTER(ctypes.c_uint), ctypes.POINTER(ctypes.c_uint))
vcount = fn("libvlc_media_player_has_vout", ctypes.c_uint, P)

args = [b"--no-video-title-show", b"--no-osd", b"--verbose=2", b"--file-logging",
        b"--logfile=" + os.path.join(out, "vlc.log").encode(), b"--no-plugins-cache",
        b"--video-x=6000", b"--video-y=6000", b"--no-video-deco", b"--width=960", b"--height=540"]
args.append(b"--bluray-menu" if mode == "menu" else b"--no-bluray-menu")
inst = new(len(args), (S * len(args))(*args))
assert inst, "libvlc_new failed"
mrl = b"bluray:///" + urllib.parse.quote(disc.replace("\\", "/")).encode()
print("mrl", mrl)
m = loc(inst, mrl)
p = mp_new(m)
print("play rc", play(p), flush=True)

def tick(label):
    print(f"[{label}] state={state(p)} t={gtime(p)}ms len={glen(p)}ms title={gtitle(p)}/{ntitle(p)} "
          f"chap={gchap(p)}/{nchap(p)} audio={naudio(p)} spu={nspu(p)} playlist={pl_id(p) if pl_id else 'n/a'} "
          f"menu_open={menu_open(p) if menu_open else 'n/a'} vout={vcount(p)}", flush=True)

ok = True
if mode == "menu":
    seen_menu = False
    for i in range(1, 31):
        time.sleep(1)
        if i % 3 == 0: tick(f"{i}s")
        if menu_open and menu_open(p) == 1: seen_menu = True
    print("menu visible before navigate_menu (first-play menu-first):", seen_menu)
    if nav_menu: nav_menu(p)
    time.sleep(3); tick("after navigate_menu")
    snap(p, 0, os.path.join(out, "menu.png").encode(), 0, 0)
    time.sleep(1)
    ok = bool(menu_open and menu_open(p) == 1)
else:
    for i in range(1, 41):
        time.sleep(1)
        if gtime(p) > 3000: break
    tick("started")
    t0 = gtime(p); time.sleep(3); t1 = gtime(p)
    print("advancing:", t1 > t0, t0, t1)
    ok = ok and t1 > t0
    n = nchap(p)
    if n > 2:
        schap(p, 2); time.sleep(3); tick("chapter 2"); ok = ok and gchap(p) in (2, 3)
    L = glen(p)
    if L > 0:
        stime(p, L // 2); time.sleep(4); tick("seek to 50%"); ok = ok and abs(gtime(p) - L // 2) < 30000
    pause(p, 1); time.sleep(1); a = gtime(p); time.sleep(2); b = gtime(p); print("paused steady:", a == b); pause(p, 0)
    time.sleep(2); tick("resumed"); ok = ok and gtime(p) > b
    at, st = ids(audio_desc(p)), ids(spu_desc(p))
    print("audio tracks", at, "current", gaudio(p)); print("spu tracks", st)
    if len(at) > 2:
        print("audio switch rc", saudio(p, at[2][0]), "now", gaudio(p)); time.sleep(3); tick("audio switched")
        ok = ok and gaudio(p) == at[2][0]
    if len(st) > 1:
        print("spu on rc", sspu(p, st[1][0])); time.sleep(3); tick("subtitle on")
    snap(p, 0, os.path.join(out, "feature.png").encode(), 0, 0)
    time.sleep(1)
tick("end")
stop(p); time.sleep(1); mp_rel(p); rel(inst)
print("RESULT", "PASS" if ok else "FAIL")
