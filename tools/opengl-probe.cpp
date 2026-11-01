// Diagnostic only: hidden legacy WGL context capability, no rendering fallback.
#include <windows.h>
#include <GL/gl.h>
#include <stdio.h>

static void json_text(const char* text) {
    if (!text) { printf("null"); return; }
    putchar('"');
    for (const unsigned char* p = (const unsigned char*)text; *p; ++p) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 32) printf("\\u%04x", *p);
        else putchar(*p);
    }
    putchar('"');
}
int main() {
    HINSTANCE instance = GetModuleHandleW(nullptr);
    WNDCLASSW wc{}; wc.style = CS_OWNDC; wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = instance; wc.lpszClassName = L"MediaBrowserOwnedGLProbe";
    if (!RegisterClassW(&wc)) return 2;
    HWND window = CreateWindowW(wc.lpszClassName, L"Media browser GL capability probe", WS_OVERLAPPEDWINDOW,
                                0, 0, 64, 64, nullptr, nullptr, instance, nullptr);
    HDC dc = window ? GetDC(window) : nullptr;
    PIXELFORMATDESCRIPTOR pfd{}; pfd.nSize = sizeof(pfd); pfd.nVersion = 1;
    pfd.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
    pfd.iPixelType = PFD_TYPE_RGBA; pfd.cColorBits = 24; pfd.cDepthBits = 24;
    int format = dc ? ChoosePixelFormat(dc, &pfd) : 0;
    bool pixel_ok = format && SetPixelFormat(dc, format, &pfd);
    HGLRC context = pixel_ok ? wglCreateContext(dc) : nullptr;
    bool current = context && wglMakeCurrent(dc, context);
    DWORD last_error = current ? 0 : GetLastError();
    const char* version = current ? (const char*)glGetString(GL_VERSION) : nullptr;
    printf("{\"scope\":\"Hidden legacy WGL capability probe, not app FBO or performance\",\"context_created\":%s,\"win32_error\":%lu,\"version\":", current ? "true" : "false", last_error);
    json_text(version); printf(",\"vendor\":"); json_text(current ? (const char*)glGetString(GL_VENDOR) : nullptr);
    printf(",\"renderer\":"); json_text(current ? (const char*)glGetString(GL_RENDERER) : nullptr);
    int major = 0; if (version) sscanf_s(version, "%d", &major);
    printf(",\"at_least_opengl_2\":%s}\n", major >= 2 ? "true" : "false");
    if (current) wglMakeCurrent(nullptr, nullptr);
    if (context) wglDeleteContext(context);
    if (dc) ReleaseDC(window, dc);
    if (window) DestroyWindow(window);
    UnregisterClassW(wc.lpszClassName, instance);
    return current ? 0 : 1;
}
