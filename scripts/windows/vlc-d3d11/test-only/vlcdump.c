/*
 * Validation harness for the Windows D3D11 video output (test tool, never shipped).
 *
 * Loads libvlc.dll from a given runtime directory exactly like Nebulae does (libvlc_new,
 * libvlc_media_player_set_hwnd on a host window), plays a clip, and either
 *   - waits for the D3D11 test-dump variant of the plugin to write its frames
 *     (NEBULAE_D3D11_TEST_DUMP*, see d3d11-test-dump.patch), or
 *   - pauses and captures the host window with PrintWindow(PW_RENDERFULLCONTENT), which works on
 *     the production plugin and returns exactly what DWM composes for that window.
 *
 * Build: x86_64-w64-mingw32-gcc -O2 -o vlcdump.exe vlcdump.c -lgdi32 -luser32
 *
 * vlcdump.exe --vlc DIR --clip FILE --log FILE [--capture OUT.bmp --at-ms 1500] [--dump-frames N]
 *             [--x X --y Y --w W --h H] [--timeout-ms MS] [--arg VLCARG]...
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef PW_RENDERFULLCONTENT
#define PW_RENDERFULLCONTENT 2
#endif

typedef void *(*p_new)(int, const char *const *);
typedef void (*p_release)(void *);
typedef void *(*p_media_new_path)(void *, const char *);
typedef void (*p_media_release)(void *);
typedef void *(*p_mp_new_from_media)(void *);
typedef void (*p_mp_release)(void *);
typedef void (*p_mp_set_hwnd)(void *, void *);
typedef int (*p_mp_play)(void *);
typedef void (*p_mp_stop)(void *);
typedef void (*p_mp_set_pause)(void *, int);
typedef int64_t (*p_mp_get_time)(void *);
typedef int (*p_mp_get_state)(void *);
typedef void (*p_log_cb)(void *, int, const void *, const char *, va_list);
typedef void (*p_log_set)(void *, p_log_cb, void *);

static FILE *g_log;

static void log_cb(void *data, int level, const void *ctx, const char *fmt, va_list args)
{
    (void)data; (void)ctx;
    if (!g_log) return;
    fprintf(g_log, "[%d] ", level);
    vfprintf(g_log, fmt, args);
    fputc('\n', g_log);
    fflush(g_log);
}

static LONG WINAPI crash_filter(EXCEPTION_POINTERS *ep)
{
    char mod[MAX_PATH] = "?";
    HMODULE m = NULL;
    void *addr = ep->ExceptionRecord->ExceptionAddress;
    if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, (LPCSTR)addr, &m))
        GetModuleFileNameA(m, mod, sizeof(mod));
    fprintf(stderr, "CRASH code=0x%08lx addr=%p module=%s rva=0x%llx info0=%llu info1=%p\n",
            ep->ExceptionRecord->ExceptionCode, addr, mod,
            (unsigned long long)((char *)addr - (char *)m),
            ep->ExceptionRecord->NumberParameters > 0 ? (unsigned long long)ep->ExceptionRecord->ExceptionInformation[0] : 0ULL,
            ep->ExceptionRecord->NumberParameters > 1 ? (void *)ep->ExceptionRecord->ExceptionInformation[1] : NULL);
    fprintf(stderr, "  rcx=%p rdx=%p r8=%p r9=%p rbx=%p r12=%p rdi=%p rsi=%p\n", (void*)ep->ContextRecord->Rcx, (void*)ep->ContextRecord->Rdx, (void*)ep->ContextRecord->R8, (void*)ep->ContextRecord->R9, (void*)ep->ContextRecord->Rbx, (void*)ep->ContextRecord->R12, (void*)ep->ContextRecord->Rdi, (void*)ep->ContextRecord->Rsi);
    void *frames[32];
    USHORT nf = RtlCaptureStackBackTrace(0, 32, frames, NULL);
    for (USHORT i = 0; i < nf; i++)
    {
        HMODULE fm = NULL; char fn[MAX_PATH] = "?";
        if (GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, (LPCSTR)frames[i], &fm))
            GetModuleFileNameA(fm, fn, sizeof(fn));
        const char *base = strrchr(fn, '\\');
        fprintf(stderr, "  #%u %s+0x%llx\n", i, base ? base + 1 : fn, (unsigned long long)((char *)frames[i] - (char *)fm));
    }
    fflush(stderr);
    return EXCEPTION_EXECUTE_HANDLER;
}

static LRESULT CALLBACK wndproc(HWND h, UINT m, WPARAM w, LPARAM l)
{
    return DefWindowProcA(h, m, w, l);
}

static void pump(DWORD ms)
{
    DWORD end = GetTickCount() + ms;
    MSG msg;
    while (GetTickCount() < end)
    {
        while (PeekMessageA(&msg, NULL, 0, 0, PM_REMOVE)) { TranslateMessage(&msg); DispatchMessageA(&msg); }
        Sleep(10);
    }
}

static int count_dumps(const char *dir)
{
    char pat[MAX_PATH + 16];
    WIN32_FIND_DATAA fd;
    int n = 0;
    snprintf(pat, sizeof(pat), "%s\\frame_*.raw", dir);
    HANDLE h = FindFirstFileA(pat, &fd);
    if (h == INVALID_HANDLE_VALUE) return 0;
    do n++; while (FindNextFileA(h, &fd));
    FindClose(h);
    return n;
}

static int capture_window(HWND hwnd, int w, int h, const char *path)
{
    HDC wdc = GetDC(hwnd);
    HDC mdc = CreateCompatibleDC(wdc);
    BITMAPINFO bi = {0};
    bi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = -h;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 32;
    bi.bmiHeader.biCompression = BI_RGB;
    void *bits = NULL;
    HBITMAP bmp = CreateDIBSection(wdc, &bi, DIB_RGB_COLORS, &bits, NULL, 0);
    HGDIOBJ old = SelectObject(mdc, bmp);
    BOOL ok = PrintWindow(hwnd, mdc, PW_RENDERFULLCONTENT);
    FILE *f = fopen(path, "wb");
    if (ok && f)
    {
        /* raw BGRA, top-down; size in the file name is carried by the caller */
        fwrite(bits, 4, (size_t)w * h, f);
    }
    if (f) fclose(f);
    SelectObject(mdc, old);
    DeleteObject(bmp);
    DeleteDC(mdc);
    ReleaseDC(hwnd, wdc);
    return ok ? 0 : 1;
}

int main(int argc, char **argv)
{
    const char *vlcdir = NULL, *clip = NULL, *logpath = NULL, *capture = NULL, *dumpdir = NULL;
    int at_ms = 1500, dump_frames = 0, x = -32000, y = 0, w = 960, h = 540, timeout_ms = 60000;
    const char *extra[32]; int nextra = 0;
    for (int i = 1; i < argc; i++)
    {
        if (!strcmp(argv[i], "--vlc") && i + 1 < argc) vlcdir = argv[++i];
        else if (!strcmp(argv[i], "--clip") && i + 1 < argc) clip = argv[++i];
        else if (!strcmp(argv[i], "--log") && i + 1 < argc) logpath = argv[++i];
        else if (!strcmp(argv[i], "--capture") && i + 1 < argc) capture = argv[++i];
        else if (!strcmp(argv[i], "--at-ms") && i + 1 < argc) at_ms = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--dump-frames") && i + 1 < argc) dump_frames = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--dump-dir") && i + 1 < argc) dumpdir = argv[++i];
        else if (!strcmp(argv[i], "--x") && i + 1 < argc) x = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--y") && i + 1 < argc) y = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--w") && i + 1 < argc) w = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--h") && i + 1 < argc) h = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--timeout-ms") && i + 1 < argc) timeout_ms = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--arg") && i + 1 < argc && nextra < 32) extra[nextra++] = argv[++i];
        else { fprintf(stderr, "bad argument %s\n", argv[i]); return 2; }
    }
    if (!vlcdir || !clip) { fprintf(stderr, "--vlc and --clip are required\n"); return 2; }
    SetUnhandledExceptionFilter(crash_filter);
    if (logpath) g_log = fopen(logpath, "w");

    char path[MAX_PATH * 2];
    snprintf(path, sizeof(path), "%s\\libvlc.dll", vlcdir);
    SetDllDirectoryA(vlcdir);
    HMODULE lib = LoadLibraryA(path);
    if (!lib) { fprintf(stderr, "cannot load %s (%lu)\n", path, GetLastError()); return 3; }
#define SYM(t, n) t n = (t)GetProcAddress(lib, #n); if (!n) { fprintf(stderr, "missing " #n "\n"); return 3; }
    SYM(p_new, libvlc_new) SYM(p_release, libvlc_release) SYM(p_media_new_path, libvlc_media_new_path)
    SYM(p_media_release, libvlc_media_release) SYM(p_mp_new_from_media, libvlc_media_player_new_from_media)
    SYM(p_mp_release, libvlc_media_player_release) SYM(p_mp_set_hwnd, libvlc_media_player_set_hwnd)
    SYM(p_mp_play, libvlc_media_player_play) SYM(p_mp_stop, libvlc_media_player_stop)
    SYM(p_mp_set_pause, libvlc_media_player_set_pause) SYM(p_mp_get_time, libvlc_media_player_get_time)
    SYM(p_mp_get_state, libvlc_media_player_get_state) SYM(p_log_set, libvlc_log_set)

    WNDCLASSA wc = {0};
    wc.lpfnWndProc = wndproc;
    wc.hInstance = GetModuleHandleA(NULL);
    wc.lpszClassName = "NebulaeVlcDump";
    wc.hbrBackground = (HBRUSH)GetStockObject(BLACK_BRUSH);
    RegisterClassA(&wc);
    HWND hwnd = CreateWindowExA(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, "NebulaeVlcDump", "vlcdump", WS_POPUP | WS_VISIBLE,
                                x, y, w, h, NULL, NULL, wc.hInstance, NULL);
    if (!hwnd) { fprintf(stderr, "CreateWindow failed\n"); return 4; }
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);

    char logv[32] = "--verbose=2";
    const char *args[48]; int n = 0;
    args[n++] = "--no-audio"; args[n++] = "--no-osd"; args[n++] = "--no-video-title-show";
    args[n++] = "--no-mouse-events"; args[n++] = "--no-keyboard-events"; args[n++] = "--no-snapshot-preview";
    args[n++] = "--avcodec-hw=none"; args[n++] = logv;
    for (int i = 0; i < nextra; i++) args[n++] = extra[i];

    void *inst = libvlc_new(n, args);
    if (!inst) { fprintf(stderr, "libvlc_new failed\n"); return 5; }
    libvlc_log_set(inst, log_cb, NULL);
    void *media = libvlc_media_new_path(inst, clip);
    void *mp = libvlc_media_player_new_from_media(media);
    libvlc_media_release(media);
    libvlc_media_player_set_hwnd(mp, hwnd);
    if (libvlc_media_player_play(mp) != 0) { fprintf(stderr, "play failed\n"); return 6; }

    DWORD start = GetTickCount();
    int rc = 0;
    if (capture)
    {
        while (libvlc_media_player_get_time(mp) < at_ms && (int)(GetTickCount() - start) < timeout_ms) pump(50);
        libvlc_media_player_set_pause(mp, 1);
        pump(800);
        RECT rc2; GetClientRect(hwnd, &rc2);
        char name[MAX_PATH * 2];
        snprintf(name, sizeof(name), "%s", capture);
        rc = capture_window(hwnd, rc2.right, rc2.bottom, name);
        fprintf(stderr, "captured %ldx%ld rc=%d t=%lld\n", rc2.right, rc2.bottom, rc, (long long)libvlc_media_player_get_time(mp));
    }
    else if (dump_frames > 0 && dumpdir)
    {
        while (count_dumps(dumpdir) < dump_frames && (int)(GetTickCount() - start) < timeout_ms) pump(100);
        rc = count_dumps(dumpdir) >= dump_frames ? 0 : 7;
    }
    else
    {
        pump(3000);
    }
    libvlc_media_player_stop(mp);
    pump(200);
    libvlc_media_player_release(mp);
    libvlc_release(inst);
    if (g_log) fclose(g_log);
    DestroyWindow(hwnd);
    ExitProcess((UINT)rc);
    return rc;
}
