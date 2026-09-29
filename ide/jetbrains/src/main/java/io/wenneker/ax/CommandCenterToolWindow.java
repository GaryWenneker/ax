package io.wenneker.ax;

import com.intellij.ide.BrowserUtil;
import com.intellij.openapi.application.ApplicationManager;
import com.intellij.openapi.project.DumbAware;
import com.intellij.openapi.project.Project;
import com.intellij.openapi.wm.ToolWindow;
import com.intellij.openapi.wm.ToolWindowFactory;
import com.intellij.ui.content.Content;
import com.intellij.ui.content.ContentFactory;
import com.intellij.ui.jcef.JBCefApp;
import com.intellij.ui.jcef.JBCefBrowser;
import java.awt.BorderLayout;
import java.io.IOException;
import java.net.HttpURLConnection;
import java.net.URI;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.SwingConstants;
import org.jetbrains.annotations.NotNull;

public final class CommandCenterToolWindow implements ToolWindowFactory, DumbAware {
    private static final int START_TIMEOUT_MS = 15_000;

    @Override
    public void createToolWindowContent(@NotNull Project project, @NotNull ToolWindow toolWindow) {
        int port = CommandCenter.DEFAULT_PORT;
        String url = CommandCenter.url(port);
        JPanel root = new JPanel(new BorderLayout());
        root.add(new JLabel("Starting ax web…", SwingConstants.CENTER), BorderLayout.CENTER);
        Content content = ContentFactory.getInstance().createContent(root, "", false);
        toolWindow.getContentManager().addContent(content);

        ApplicationManager.getApplication().executeOnPooledThread(() -> {
            String error = ensureServer(port, project.getBasePath());
            ApplicationManager.getApplication().invokeLater(() -> {
                root.removeAll();
                root.add(error == null && JBCefApp.isSupported() ? browser(url, content) : fallback(url, error), BorderLayout.CENTER);
                root.revalidate();
                root.repaint();
            });
        });
    }

    private static JComponent browser(String url, Content content) {
        JBCefBrowser browser = new JBCefBrowser(url);
        content.setDisposer(browser);
        return browser.getComponent();
    }

    private static JComponent fallback(String url, String error) {
        JPanel panel = new JPanel(new BorderLayout());
        String text = error != null ? "ax web did not start: " + error : "This IDE has no embedded browser.";
        panel.add(new JLabel(text, SwingConstants.CENTER), BorderLayout.CENTER);
        JButton open = new JButton("Open in browser");
        open.addActionListener(e -> BrowserUtil.browse(url));
        panel.add(open, BorderLayout.SOUTH);
        return panel;
    }

    private static boolean isUp(int port) {
        try {
            HttpURLConnection c = (HttpURLConnection) URI.create("http://127.0.0.1:" + port + "/").toURL().openConnection();
            c.setConnectTimeout(1500);
            c.setReadTimeout(1500);
            return c.getResponseCode() == 200;
        } catch (IOException e) {
            return false;
        }
    }

    /** Null when ax web answers; otherwise the reason it did not. */
    private static String ensureServer(int port, String projectDir) {
        if (isUp(port)) {
            return null;
        }
        try {
            new ProcessBuilder(CommandCenter.startCommand("ax", port, projectDir)).inheritIO().start();
        } catch (IOException e) {
            return e.getMessage();
        }
        long deadline = System.currentTimeMillis() + START_TIMEOUT_MS;
        while (System.currentTimeMillis() < deadline) {
            if (isUp(port)) {
                return null;
            }
            try {
                Thread.sleep(500);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return "interrupted";
            }
        }
        return "no answer on port " + port + " after " + START_TIMEOUT_MS / 1000 + " s";
    }
}
