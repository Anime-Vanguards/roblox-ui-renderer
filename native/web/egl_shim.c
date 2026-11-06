/* Emscripten's EGL is 1.4; wgpu-hal's GLES backend links khronos-egl with the EGL 1.5 entry points
 * (it calls them only for Wayland / X11 displays, never on Emscripten). Forward them to the 1.4 calls
 * so the module links. */
#include <EGL/egl.h>
#include <stddef.h>
#include <stdint.h>

typedef intptr_t EGLAttrib;

EGLDisplay eglGetPlatformDisplay(EGLenum platform, void *native_display, const EGLAttrib *attrib_list) {
    (void)platform;
    (void)attrib_list;
    return eglGetDisplay((EGLNativeDisplayType)native_display);
}

EGLSurface eglCreatePlatformWindowSurface(EGLDisplay display, EGLConfig config, void *native_window,
                                          const EGLAttrib *attrib_list) {
    (void)attrib_list;
    return eglCreateWindowSurface(display, config, (EGLNativeWindowType)native_window, NULL);
}
