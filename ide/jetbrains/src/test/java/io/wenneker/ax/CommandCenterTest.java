package io.wenneker.ax;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertThrows;

import java.util.List;
import org.junit.Test;

public class CommandCenterTest {
    @Test
    public void urlIsEmbedModeOnLoopback() {
        assertEquals("http://127.0.0.1:7071/?embed=1", CommandCenter.url(7071));
    }

    @Test
    public void urlRejectsPortOutOfRange() {
        assertThrows(IllegalArgumentException.class, () -> CommandCenter.url(0));
        assertThrows(IllegalArgumentException.class, () -> CommandCenter.url(65536));
    }

    @Test
    public void startCommandAddsProjectDirOnlyWhenPresent() {
        assertEquals(List.of("ax", "web", "--port", "7070", "/p"), CommandCenter.startCommand("ax", 7070, "/p"));
        assertEquals(List.of("ax", "web", "--port", "7070"), CommandCenter.startCommand("ax", 7070, null));
        assertEquals(List.of("ax", "web", "--port", "7070"), CommandCenter.startCommand("ax", 7070, " "));
    }
}
