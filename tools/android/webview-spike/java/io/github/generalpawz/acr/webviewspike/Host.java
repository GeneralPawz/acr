package io.github.generalpawz.acr.webviewspike;

import android.app.Activity;
import android.content.Intent;
import android.graphics.Color;
import android.util.DisplayMetrics;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.webkit.JavascriptInterface;
import android.webkit.WebView;
import android.widget.PopupWindow;

/**
 * The spike's whole Java side (docs/webui.md, "Android spike"): a transparent WebView in a
 * PopupWindow over the NativeActivity's window. The Rust example drives it through JNI and
 * receives messages and forwarded touches through the two native methods.
 */
public final class Host {
    /** A message from the page (JSON text), on a WebView binder thread. */
    public static native void nativeMessage(String json);

    /** A touch outside every region the page declared, forwarded for the game to handle. */
    public static native void nativeTouch(int action, float x, float y);

    private static Activity activity;
    private static WebView web;
    private static PopupWindow popup;
    private static float density = 1f;
    /** Interactive rectangles in CSS pixels, {x, y, w, h}; set by the page. */
    private static volatile float[][] regions = new float[0][];
    private static boolean forward;
    private static boolean owned;

    /** The intent extra {@code key} of the activity ("" when absent). */
    public static String extra(Activity a, String key) {
        Intent i = a.getIntent();
        String v = i == null ? null : i.getStringExtra(key);
        return v == null ? "" : v;
    }

    /** What the page calls as {@code acr.post(json)} and {@code acr.regions(csv)}. */
    public static final class Bridge {
        @JavascriptInterface
        public void post(String json) {
            nativeMessage(json);
        }

        @JavascriptInterface
        public void regions(String csv) {
            String[] rects = csv.isEmpty() ? new String[0] : csv.split(";");
            float[][] out = new float[rects.length][4];
            for (int i = 0; i < rects.length; i++) {
                String[] n = rects[i].split(",");
                for (int k = 0; k < 4; k++) out[i][k] = Float.parseFloat(n[k]);
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

    /**
     * Shows the page. {@code mode}: {@code full} (full-screen popup, touches outside the page's
     * regions go to the game), {@code capture} (full-screen, the popup keeps every touch) or
     * {@code shrink} (the popup is only the middle half of the screen).
     */
    public static void attach(final Activity a, final String html, final String mode) {
        activity = a;
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                DisplayMetrics m = new DisplayMetrics();
                a.getWindowManager().getDefaultDisplay().getRealMetrics(m);
                density = m.density;
                forward = mode.equals("full");
                web = new WebView(a);
                web.setBackgroundColor(Color.TRANSPARENT);
                web.setLayerType(View.LAYER_TYPE_HARDWARE, null);
                web.getSettings().setJavaScriptEnabled(true);
                web.addJavascriptInterface(new Bridge(), "acr");
                web.setOnTouchListener(new View.OnTouchListener() {
                    @Override
                    public boolean onTouch(View v, MotionEvent e) {
                        int act = e.getActionMasked();
                        if (act == MotionEvent.ACTION_DOWN) {
                            owned = !forward || inRegion(e.getX() / density, e.getY() / density);
                        }
                        if (owned) return false;
                        nativeTouch(act, e.getRawX(), e.getRawY());
                        return true;
                    }
                });
                int x = 0, y = 0, w = m.widthPixels, h = m.heightPixels;
                if (mode.equals("shrink")) {
                    x = w / 4;
                    y = h / 4;
                    w /= 2;
                    h /= 2;
                }
                // Not focusable (the activity keeps the keys), touchable, outside touches go
                // through to the window below, drawn over the cutout and the bars.
                popup = new PopupWindow(web, w, h, false);
                popup.setClippingEnabled(false);
                popup.setOutsideTouchable(true);
                popup.showAtLocation(a.getWindow().getDecorView(), Gravity.TOP | Gravity.START, x, y);
                web.loadDataWithBaseURL("https://acr.invalid/", html, "text/html", "utf-8", null);
            }
        });
    }

    /** Runs {@code js} in the page (the UI thread does the call). */
    public static void eval(final String js) {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                web.evaluateJavascript(js, null);
            }
        });
    }

    public static void detach() {
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (popup != null) popup.dismiss();
                if (web != null) web.destroy();
                popup = null;
                web = null;
            }
        });
    }
}
