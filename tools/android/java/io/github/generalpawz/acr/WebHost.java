package io.github.generalpawz.acr;

import android.app.Activity;
import android.graphics.Color;
import android.net.Uri;
import android.util.DisplayMetrics;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;
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
    private static float density = 1f;
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
                final View decor = a.getWindow().getDecorView();
                if (decor.getWindowToken() == null) {
                    // The popup needs the activity's window token; wait for the window.
                    decor.postDelayed(this, 50);
                    return;
                }
                DisplayMetrics m = new DisplayMetrics();
                a.getWindowManager().getDefaultDisplay().getRealMetrics(m);
                density = m.density;
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
                web.addJavascriptInterface(new Bridge(), "acr");
                web.setWebViewClient(new WebViewClient() {
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
                // through to the window below, drawn over the cutout and the bars.
                popup = new PopupWindow(web, m.widthPixels, m.heightPixels, false);
                popup.setClippingEnabled(false);
                popup.setOutsideTouchable(true);
                popup.showAtLocation(decor, Gravity.TOP | Gravity.START, 0, 0);
                web.loadUrl(url);
            }
        });
    }

    /** Runs {@code js} in the page (the UI thread does the call). */
    public static void eval(final String js) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (web != null) web.evaluateJavascript(js, null);
            }
        });
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
