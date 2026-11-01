package io.github.generalpawz.acr;

import android.app.Activity;
import android.os.Build;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * What the Rust side needs from the activity's Java side, whether or not the Svelte UI's WebView
 * exists (docs/android.md): the back gesture and the screen orientation. Its native is registered
 * by {@code android.rs} with {@code RegisterNatives}, like {@link WebHost}'s.
 */
public final class Shell {
    private Shell() {}

    /** The back gesture or key arrived; Rust turns it into Escape. */
    public static native void nativeBack();

    private static OnBackInvokedCallback callback;

    /**
     * Makes the system's back (gesture, button, {@code KEYCODE_BACK}) call {@link #nativeBack}
     * instead of finishing the activity. API 33+: there the key no longer reaches the app and
     * the unhandled back finishes the activity; before, the back key arrives as a key event.
     */
    public static void installBack(final Activity a) {
        if (Build.VERSION.SDK_INT < 33) return;
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                if (callback != null) return;
                callback = new OnBackInvokedCallback() {
                    @Override
                    public void onBackInvoked() {
                        nativeBack();
                    }
                };
                a.getOnBackInvokedDispatcher()
                        .registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            }
        });
    }

    /** {@code Activity.setRequestedOrientation(orientation)} on the UI thread. */
    public static void setOrientation(final Activity a, final int orientation) {
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                a.setRequestedOrientation(orientation);
            }
        });
    }
}
