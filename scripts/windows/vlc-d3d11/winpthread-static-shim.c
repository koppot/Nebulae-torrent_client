/*
 * libstdc++ from the posix-threads mingw-w64 toolchain references winpthread
 * through __imp_ pointers, which would make the plugin import
 * libwinpthread-1.dll (not part of the VLC runtime). Resolve those pointers to
 * the statically linked libwinpthread.a instead.
 */
extern int pthread_key_create(), pthread_mutex_destroy(), pthread_mutex_init(),
    pthread_mutex_lock(), pthread_mutex_unlock(), pthread_once(), pthread_setspecific();
extern void *pthread_getspecific();
#define SHIM(n) void *__imp_##n = (void *)&n;
SHIM(pthread_getspecific) SHIM(pthread_key_create) SHIM(pthread_mutex_destroy)
SHIM(pthread_mutex_init) SHIM(pthread_mutex_lock) SHIM(pthread_mutex_unlock)
SHIM(pthread_once) SHIM(pthread_setspecific)
