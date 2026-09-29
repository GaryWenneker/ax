package io.wenneker.ax;

import java.util.ArrayList;
import java.util.List;

/** IDE-independent parts of the tool window, so they can be tested without a running IDE. */
public final class CommandCenter {
    public static final int DEFAULT_PORT = 7070;

    private CommandCenter() {}

    public static String url(int port) {
        if (port < 1 || port > 65535) {
            throw new IllegalArgumentException("port must be between 1 and 65535, got " + port);
        }
        return "http://127.0.0.1:" + port + "/?embed=1";
    }

    /** `ax web --port N [projectDir]`; the project dir is omitted when the IDE has none. */
    public static List<String> startCommand(String binary, int port, String projectDir) {
        List<String> cmd = new ArrayList<>(List.of(binary, "web", "--port", String.valueOf(port)));
        if (projectDir != null && !projectDir.isBlank()) {
            cmd.add(projectDir);
        }
        return cmd;
    }
}
