"""Windowed -> fullscreen -> windowed -> resize on a staged runtime (test-only). usage: fs_test.py RESOURCES_DIR CLIP OUT_DIR"""
import ctypes, os, sys, time
res, clip, out = sys.argv[1:4]
os.makedirs(out, exist_ok=True)
vd = os.path.join(res, "vlc")
os.environ["VLC_PLUGIN_PATH"] = os.path.join(vd, "plugins"); os.environ["PATH"] = vd + os.pathsep + os.environ["PATH"]
os.add_dll_directory(vd)
v = ctypes.CDLL(os.path.join(vd, "libvlc.dll"))
P = ctypes.c_void_p
for n, r, a in [("libvlc_new", P, [ctypes.c_int, ctypes.POINTER(ctypes.c_char_p)]), ("libvlc_media_new_path", P, [P, ctypes.c_char_p]),
                ("libvlc_media_player_new_from_media", P, [P]), ("libvlc_media_player_play", ctypes.c_int, [P]),
                ("libvlc_media_player_get_time", ctypes.c_longlong, [P]), ("libvlc_media_player_get_state", ctypes.c_int, [P]),
                ("libvlc_set_fullscreen", None, [P, ctypes.c_int]), ("libvlc_get_fullscreen", ctypes.c_int, [P]),
                ("libvlc_media_player_stop", None, [P]), ("libvlc_media_player_release", None, [P]), ("libvlc_release", None, [P])]:
    f = getattr(v, n); f.restype = r; f.argtypes = a
args = [b"--no-audio", b"--no-osd", b"--no-video-title-show", b"--avcodec-hw=none", b"--verbose=2", b"--file-logging",
        b"--logfile=" + os.path.join(out, "vlc.log").encode(), b"--no-plugins-cache", b"--input-repeat=50", b"--no-video-deco", b"--width=960", b"--height=540"]
inst = v.libvlc_new(len(args), (ctypes.c_char_p * len(args))(*args))
p = v.libvlc_media_player_new_from_media(v.libvlc_media_new_path(inst, clip.encode()))
v.libvlc_media_player_play(p)
def step(label, secs=4):
    time.sleep(secs); print(f"{label}: state={v.libvlc_media_player_get_state(p)} t={v.libvlc_media_player_get_time(p)} fs={v.libvlc_get_fullscreen(p)}", flush=True)
states = []
def step(label, secs=4, _s=step):
    _s(label, secs); states.append(v.libvlc_media_player_get_state(p))
step("windowed", 6)
v.libvlc_set_fullscreen(p, 1); step("fullscreen")
v.libvlc_set_fullscreen(p, 0); step("windowed again")
v.libvlc_set_fullscreen(p, 1); step("fullscreen 2", 3)
v.libvlc_set_fullscreen(p, 0); step("windowed 2", 3)
ok = all(x == 3 for x in states)
v.libvlc_media_player_stop(p); time.sleep(1); v.libvlc_media_player_release(p); v.libvlc_release(inst)
print("RESULT", "PASS" if ok else "FAIL")
