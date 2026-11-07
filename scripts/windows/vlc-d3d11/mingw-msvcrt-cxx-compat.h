/* libstdc++ (mingw-w64 16.2) believes quick_exit exists, but msvcrt.dll does not export it. Declaration only; never referenced. */
#ifdef __cplusplus
extern "C" {
void quick_exit(int) __attribute__((noreturn));
int at_quick_exit(void (*)(void));
}
#endif
