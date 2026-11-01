package io.github.generalpawz.acr;

import android.app.Activity;
import android.content.pm.ApplicationInfo;
import android.graphics.Color;
import android.net.Uri;
import android.graphics.Insets;
import android.os.Build;
import android.util.Log;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.DisplayCutout;
import android.view.View;
import android.view.WindowInsets;
import android.webkit.ConsoleMessage;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebChromeClient;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.webkit.WebSettings;
import android.widget.PopupWindow;

import java.io.ByteArrayInputStream;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * The Java side of the Svelte UI on Android (docs/webui.md, "Native host (Android, #479)"): a
 * transparent WebView in a PopupWindow over the NativeActivity's window. The Rust half is
 * {@code crates/acr-game/src/webui/android.rs}, which registers the three native methods.
 */
public final class WebHost {
    /** A message from the page (JSON text), on a WebView binder thread. */
    public static native void nativeMessage(String json);

    /** A finger the page did not claim. {@code phase}: 0 started, 1 moved, 2 ended, 3 cancelled. */
    public static native void nativeTouch(int phase, int id, float x, float y);

    /** The page asks for {@code path} of https://acr.local; Rust answers with {@link #reply}. */
    public static native void nativeRequest(String path, int id);

    private static final String HOST = "acr.local";

    private static Activity activity;
    private static WebView web;
    private static PopupWindow popup;
    private static View decor;
    private static float density = 1f;
    /** The popup's size and the insets last applied, in pixels: {width, height, left, top, right, bottom}. */
    private static final int[] laidOut = new int[6];
    /** The app is in the foreground with a window: nothing runs in the page otherwise. */
    private static volatile boolean active = true;
    /** Interactive rectangles in CSS pixels, {x, y, w, h}; set by the page. */
    private static volatile float[][] regions = new float[0][];
    /** The page owns the current gesture (it started inside a region). */
    private static boolean owned;

    private static final AtomicInteger nextId = new AtomicInteger();
    private static final Map<Integer, Pending> pending = new ConcurrentHashMap<>();

    /** One request the WebView's IO thread waits on. */
    private static final class Pending {
        final CountDownLatch done = new CountDownLatch(1);
        int status;
        String mime;
        byte[] body;
        String error;
    }

    /** What the page calls as {@code acr.post(json)} and {@code acr.regions(csv)}. */
    public static final class Bridge {
        @JavascriptInterface
        public void post(String json) {
            nativeMessage(json);
        }

        /** The page's interactive rectangles: {@code x,y,w,h;x,y,w,h...} in CSS pixels. */
        @JavascriptInterface
        public void regions(String csv) {
            String[] rects = csv.isEmpty() ? new String[0] : csv.split(";");
            float[][] out = new float[rects.length][4];
            try {
                for (int i = 0; i < rects.length; i++) {
                    String[] n = rects[i].split(",");
                    for (int k = 0; k < 4; k++) out[i][k] = Float.parseFloat(n[k]);
                }
            } catch (RuntimeException e) {
                // A malformed list claims nothing: the game gets the touches.
                out = new float[0][];
            }
            regions = out;
        }
    }

    private static boolean inRegion(float x, float y) {
        for (float[] r : regions) {
            if (x >= r[0] && x < r[0] + r[2] && y >= r[1] && y < r[1] + r[3]) return true;
        }
        return false;
    }

    /** Hands the fingers of {@code e} that the page does not own to the game. */
    private static void forward(MotionEvent e) {
        int index = e.getActionIndex();
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
            case MotionEvent.ACTION_POINTER_DOWN:
                nativeTouch(0, e.getPointerId(index), e.getX(index), e.getY(index));
                break;
            case MotionEvent.ACTION_MOVE:
                for (int i = 0; i < e.getPointerCount(); i++) {
                    nativeTouch(1, e.getPointerId(i), e.getX(i), e.getY(i));
                }
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_POINTER_UP:
                nativeTouch(2, e.getPointerId(index), e.getX(index), e.getY(index));
                break;
            case MotionEvent.ACTION_CANCEL:
                for (int i = 0; i < e.getPointerCount(); i++) {
                    nativeTouch(3, e.getPointerId(i), e.getX(i), e.getY(i));
                }
                break;
            default:
                break;
        }
    }

    /**
     * Shows the page at {@code url} (an https://acr.local URL, answered by Rust through
     * {@link #nativeRequest}) in a full-screen popup. A popup is a window of its own above the
     * activity's surface; touches outside the page's regions go to the game.
     */
    public static void attach(final Activity a, final String url) {
        activity = a;
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                decor = a.getWindow().getDecorView();
                if (decor.getWindowToken() == null || decor.getWidth() == 0) {
                    // The popup needs the activity's window token and its size; wait for the window.
                    decor.postDelayed(this, 50);
                    return;
                }
                web = new WebView(a);
                web.setBackgroundColor(Color.TRANSPARENT);
                web.setLayerType(View.LAYER_TYPE_HARDWARE, null);
                web.setVerticalScrollBarEnabled(false);
                web.setHorizontalScrollBarEnabled(false);
                web.setOverScrollMode(View.OVER_SCROLL_NEVER);
                web.setLongClickable(false);
                web.setHapticFeedbackEnabled(false);
                web.getSettings().setJavaScriptEnabled(true);
                web.getSettings().setAllowFileAccess(false);
                web.getSettings().setAllowContentAccess(false);
                web.getSettings().setSupportZoom(false);
                // No keys, no soft keyboard: the activity keeps the focus and the page has no input.
                web.setFocusable(false);
                web.setFocusableInTouchMode(false);
                web.setImportantForAutofill(View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
                web.getSettings().setCacheMode(WebSettings.LOAD_NO_CACHE);
                web.addJavascriptInterface(new Bridge(), "acr");
                // chrome://inspect over adb, for a debuggable build only.
                WebView.setWebContentsDebuggingEnabled((a.getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0);
                web.setWebChromeClient(new WebChromeClient() {
                    @Override
                    public boolean onConsoleMessage(ConsoleMessage m) {
                        Log.println(m.messageLevel() == ConsoleMessage.MessageLevel.ERROR ? Log.ERROR : Log.INFO,
                                "acr", "page: " + m.message() + " (" + m.sourceId() + ":" + m.lineNumber() + ")");
                        return true;
                    }
                });
                web.setWebViewClient(new WebViewClient() {
                    @Override
                    public void onPageFinished(WebView v, String u) {
                        // The page is there now: it learns the insets that were set before.
                        laidOut[2] = laidOut[3] = laidOut[4] = laidOut[5] = -1;
                        layout();
                    }

                    @Override
                    public WebResourceResponse shouldInterceptRequest(WebView v, WebResourceRequest r) {
                        Uri u = r.getUrl();
                        if (!"https".equals(u.getScheme()) || !HOST.equals(u.getHost())) {
                            return answer(403, "text/plain", new byte[0], "outside https://" + HOST);
                        }
                        return serve(u.getPath());
                    }
                });
                web.setOnTouchListener(new View.OnTouchListener() {
                    @Override
                    public boolean onTouch(View v, MotionEvent e) {
                        if (e.getActionMasked() == MotionEvent.ACTION_DOWN) {
                            owned = inRegion(e.getX() / density, e.getY() / density);
                        }
                        if (owned) return false;
                        forward(e);
                        return true;
                    }
                });
                // Not focusable (the activity keeps the keys), touchable, outside touches go
                // through to the window below. It is exactly the activity's window: the game's
                // surface fills it, so a touch has the same pixel in both.
                popup = new PopupWindow(web, decor.getWidth(), decor.getHeight(), false);
                popup.setClippingEnabled(false);
                popup.setOutsideTouchable(true);
                popup.setInputMethodMode(PopupWindow.INPUT_METHOD_NOT_NEEDED);
                popup.showAtLocation(decor, Gravity.TOP | Gravity.START, 0, 0);
                // Rotation, a resize, a cutout or the bars coming and going.
                decor.addOnLayoutChangeListener(new View.OnLayoutChangeListener() {
                    @Override
                    public void onLayoutChange(View v, int l, int t, int r, int b, int ol, int ot, int or, int ob) {
                        layout();
                    }
                });
                web.setOnApplyWindowInsetsListener(new View.OnApplyWindowInsetsListener() {
                    @Override
                    public WindowInsets onApplyWindowInsets(View v, WindowInsets insets) {
                        layout();
                        return v.onApplyWindowInsets(insets);
                    }
                });
                layout();
                web.loadUrl(url);
            }
        });
    }

    /** Runs {@code js} in the page (the UI thread does the call); nothing runs while the app is away. */
    public static void eval(final String js) {
        if (!active) return;
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (web != null && active) web.evaluateJavascript(js, null);
            }
        });
    }

    /**
     * The app went to the background ({@code false}: its window is gone) or came back: the page's
     * timers, animations and rendering stop, and {@link #eval} drops what Rust sends.
     */
    public static void setActive(final boolean on) {
        active = on;
        if (activity == null) return;
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (web == null) return;
                if (on) {
                    web.onResume();
                    web.resumeTimers();
                    layout();
                } else {
                    web.onPause();
                    web.pauseTimers();
                }
            }
        });
    }

    /**
     * Fits the popup to the activity's window and tells the page the insets, as CSS variables
     * {@code --acr-inset-top/right/bottom/left} on the root (CSS pixels: the system bars and the
     * display cutout over the window). On the UI thread.
     */
    private static void layout() {
        if (web == null || popup == null || decor == null) return;
        int w = decor.getWidth();
        int h = decor.getHeight();
        if (w > 0 && h > 0 && (w != laidOut[0] || h != laidOut[1])) {
            laidOut[0] = w;
            laidOut[1] = h;
            popup.update(0, 0, w, h);
        }
        density = decor.getResources().getDisplayMetrics().density;
        int[] in = insets(decor.getRootWindowInsets());
        boolean same = true;
        for (int i = 0; i < 4; i++) same &= in[i] == laidOut[2 + i];
        if (same) return;
        for (int i = 0; i < 4; i++) laidOut[2 + i] = in[i];
        String[] side = {"left", "top", "right", "bottom"};
        StringBuilder js = new StringBuilder("(function(s){");
        for (int i = 0; i < 4; i++) {
            js.append("s.setProperty('--acr-inset-").append(side[i]).append("','")
                    .append(in[i] / density).append("px');");
        }
        js.append("})(document.documentElement.style)");
        web.evaluateJavascript(js.toString(), null);
    }

    /** The bars and the cutout over the window, {left, top, right, bottom} in pixels. */
    private static int[] insets(WindowInsets wi) {
        int[] out = new int[4];
        if (wi == null) return out;
        if (Build.VERSION.SDK_INT >= 30) {
            Insets i = wi.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
            out[0] = i.left;
            out[1] = i.top;
            out[2] = i.right;
            out[3] = i.bottom;
        } else {
            out[0] = wi.getSystemWindowInsetLeft();
            out[1] = wi.getSystemWindowInsetTop();
            out[2] = wi.getSystemWindowInsetRight();
            out[3] = wi.getSystemWindowInsetBottom();
            DisplayCutout c = wi.getDisplayCutout();
            if (c != null) {
                out[0] = Math.max(out[0], c.getSafeInsetLeft());
                out[1] = Math.max(out[1], c.getSafeInsetTop());
                out[2] = Math.max(out[2], c.getSafeInsetRight());
                out[3] = Math.max(out[3], c.getSafeInsetBottom());
            }
        }
        return out;
    }

    /** Rust's answer to {@link #nativeRequest}; {@code error} is empty when there is none. */
    public static void reply(int id, int status, String mime, byte[] body, String error) {
        Pending p = pending.get(id);
        if (p == null) return;
        p.status = status;
        p.mime = mime;
        p.body = body;
        p.error = error;
        p.done.countDown();
    }

    /** Asks Rust for {@code path} and waits for the answer (the game's thread makes assets). */
    private static WebResourceResponse serve(String path) {
        int id = nextId.incrementAndGet();
        Pending p = new Pending();
        pending.put(id, p);
        try {
            nativeRequest(path, id);
            if (!p.done.await(30, TimeUnit.SECONDS)) {
                return answer(504, "text/plain", new byte[0], "the game did not answer");
            }
            return answer(p.status, p.mime, p.body, p.error);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return answer(504, "text/plain", new byte[0], "interrupted");
        } finally {
            pending.remove(id);
        }
    }

    private static WebResourceResponse answer(int status, String mime, byte[] body, String error) {
        Map<String, String> headers = new HashMap<>();
        headers.put("Cache-Control", "no-cache");
        if (error != null && !error.isEmpty()) headers.put("x-acr-error", error);
        String encoding = mime.startsWith("text/") || mime.contains("json") || mime.contains("javascript") ? "utf-8" : null;
        return new WebResourceResponse(mime, encoding, status, status == 200 ? "OK" : "Error", headers,
                new ByteArrayInputStream(body));
    }
}
