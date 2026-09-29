---
title: Obsidian Vault
description: Open your ax rules, skills, and memories in Obsidian as normal notes, served straight from ax.db over WebDAV. No note files on disk.
---

`ax web` serves your project's rules, skills, and memories as a WebDAV share at `http://127.0.0.1:7070/ax/` (the older `http://127.0.0.1:7070/dav/` still works). Mount it on macOS, Windows, or Linux and open it in Obsidian as a vault. You get the normal Obsidian editor, backlinks, search, and graph, and every save goes straight into `ax.db`. Agents see the change on their next `ax_preflight`.

## Connect from Settings

In the Command Center, open **Settings → Vault connection** and click **Connect as "ax"**. ax runs the connect command that ships with your system:

| System | What happens | Where the vault appears |
|---|---|---|
| macOS | `mount_webdav` mounts the share | `~/ax` |
| Windows | `net use` maps the first free drive letter (Z: downward) and labels it `ax` | for example `Y:` |
| Linux | `gio mount` connects it and adds an `ax` bookmark in Files | Files sidebar |

Change the **Drive name** before connecting to use another name. **Connect at login** reconnects when you log in, as long as `ax web` is running. **Disconnect** removes the drive, the bookmark, and the login item.

On Windows the **WebClient** service must be running. If it is not, the card shows the command to start it. If a connect command fails, the card shows the exact command so you can copy it and run it yourself. The buttons work only from a browser on the same machine, and never in an `ax share` session.

## Connect manually

Start the Command Center first. The share exists only while `ax web` runs:

```bash
ax web
```

Use another port with `ax web --port <port>` and change `7070` in the addresses below to match.

### macOS

In Finder, choose **Go → Connect to Server** (<kbd>⌘K</kbd>), enter `http://127.0.0.1:7070/ax/`, and click **Connect**. Choose **Guest** if Finder asks. The share mounts at `/Volumes/ax`.

From a terminal:

```bash
mkdir -p ~/ax
/sbin/mount_webdav -S -v ax http://127.0.0.1:7070/ax/ ~/ax
```

### Windows

Start the WebClient service once (as administrator), then map a drive:

```powershell
sc config WebClient start= auto
sc start WebClient
net use Y: \\127.0.0.1@7070\ax /persistent:no
```

The vault is then drive `Y:`. In Explorer you can also use **This PC → Map network drive** with the folder `\\127.0.0.1@7070\ax`.

### Linux

```bash
gio mount dav://127.0.0.1:7070/ax/
```

The share appears in the Files sidebar and under `/run/user/$UID/gvfs/`. In Files you can also use **Other Locations → Connect to Server** with `dav://127.0.0.1:7070/ax/`.

## Open it in Obsidian

1. Connect the vault (from Settings or by hand, see above).
2. In Obsidian, open the vault switcher (**Open another vault** in the bottom left) and choose **Open folder as vault**.
3. Pick the mounted folder: `~/ax` or `/Volumes/ax` on macOS, the drive letter on Windows, or the gvfs folder on Linux.
4. If Obsidian asks whether you trust the author of the vault, choose **Trust**. The vault's `.obsidian/` settings come from `ax.db`.
5. Open `rules/`, `skills/`, or `memories/` in the file explorer and edit a page. Saving writes it to `ax.db` right away. The Command Center shows the change after a refresh, and agents get it on their next `ax_preflight`.

Obsidian remembers the vault. Next time, connect the drive (or turn on **Connect at login**), start `ax web`, and open the vault from the vault switcher.

Tips:

- Use the graph view (<kbd>⌘G</kbd> / <kbd>Ctrl+G</kbd>) to see how rules, skills, and memories link to each other. See [Links](#links).
- Obsidian Sync or Git plugins are not needed. ax is the storage, and the rules keep their revision history in ax.

## What is in the vault

| Path | What it is |
|---|---|
| `rules/<id>.md` | One page per policy rule, with the same frontmatter as a `.agents/rules/*.mdc` file |
| `skills/<name>.md` | One page per skill |
| `memories/<title>.md` | One page per memory, with `id`, `kind`, `tags`, and `files` in the frontmatter |
| `global/rules/<id>.md`, `global/skills/<name>.md` | Your global rules and skills from `~/.ax/global.db`, which apply to every project |
| `DRAFTS.md` | Pages that did not save, and why (read-only) |
| `.obsidian/` | Obsidian's own settings, stored in `ax.db` |

## Editing

- **Save a page** and it goes through the same save path as the Command Center. A policy revision is recorded, and rules and skills stored as files update their `.agents/` file as before.
- **Leave out the frontmatter** and ax keeps the stored frontmatter, replacing only the body.
- **New note** in `rules/`: name it with a kebab-case id (`my-rule.md`) and type the rule. ax creates it with `level: INFO` and the id as its trigger. Edit the frontmatter to change that.
- **New note** in `memories/`: the file name becomes the memory title.
- **Rename** a page to rename the rule, skill, or memory. **Delete** a page to delete it.

### New notes land in `memories/`

A note at the vault root is kept as a plain file and does not become a rule, skill, or memory. So the vault tells Obsidian to file new notes under `memories/` (**Settings → Files and links → Default location for new notes**). This takes effect the next time you open the vault. A setting you already chose is never changed.

### When a save fails

If a page has invalid frontmatter, or its `id` does not match the file name, ax does **not** change the stored rule. Your text is kept as a draft at the same path, so nothing you typed is lost, and `DRAFTS.md` lists the page with the reason. Fix the page and save again; the draft disappears.

## Links

Link rules, skills, and memories to each other the way you link notes in Obsidian:

| You write | Links to |
|---|---|
| `[[pre-pr-check]]` | The rule, skill, or memory with that name |
| `[[skills/pr]]`, `[[rules/english-only]]`, `[[memories/Use SQLite]]` | That exact page, when a name is used in more than one place |
| `[[global/skills/review-loop]]` | A global skill (`global/rules/…` for a global rule) |
| `[[pr\|the PR skill]]`, `[[pr#Steps]]` | The same item, shown with a label or pointing at a heading |

A bare name is looked up in this order: project rule, global skill, project skill, memory, global rule. Links inside code blocks and `inline code` are ignored.

What links do in ax:

- **Agents get the linked items.** When `ax_preflight` delivers a rule, skill, or memory, it also delivers the items that one links to, one hop deep and at most five per turn. A linked rule shows up with the reason `link:rule/<id>`. Disabled or unapproved items are never delivered through a link.
- **The Command Center shows them.** Links in a rule, skill, or memory are clickable and open that item. A link to something that does not exist is shown dimmed, with the tooltip "No rule, skill or memory named …". Under the body, **Linked from** lists every item that links here.
- **Renaming in Obsidian updates links.** The vault turns on Obsidian's *Automatically update internal links*, so renaming a page rewrites the links to it in other pages, and those edits are saved like any other. Renaming in the Command Center or through MCP does not rewrite links.

## Global rules and skills

`global/rules/` and `global/skills/` show the global items from `~/.ax/global.db`. When several projects hold a copy of the same global item, you see the one that wins (the most complete copy).

- **Save** a global page to update it for every project. Invalid frontmatter becomes a draft, just like project pages.
- **New note** in `global/skills/` or `global/rules/` creates a new global item.
- **Delete, rename, and copy are refused** (403) for saved global items, so a slip in one vault cannot remove something every project uses. Delete or rename them in the Command Center. A new note that never saved can still be renamed or deleted.

## Folders

**Settings → Vault connection → Folders** adds directories from your disk to the vault. Each one appears as `folders/<name>/` and shows the real files: reading, saving, creating, deleting, and renaming inside it change the files on disk. The list is stored in `.ax/vault-folders.json`.

- **Add folder** asks for a name (1–32 letters, digits, spaces, `_` or `-`, unique) and an absolute path to an existing directory. **Choose…** opens your system's folder dialog (Finder on macOS, the folder browser on Windows, zenity or kdialog on Linux), fills the path, and suggests the folder's name. Without a dialog (for example Linux without zenity or kdialog), type the path.
- **Index into memory** imports the folder's `.md`, `.markdown`, and `.txt` files as `doc` memories. Agents find them with recall, but they are not listed in preflight's memory titles. They are re-synced when `ax web` starts, when you click **Sync now**, and shortly after you save a file through the vault. Turning indexing off, or removing the folder, deletes those memories. The files stay on disk.
- Each row shows the last sync: files added, updated, removed, and skipped.
- Only the browser on your own machine can see or change folders. Requests with another host or a foreign `Origin` get 403, including from share sessions and other devices on your network.
- `folders/` and each `folders/<name>/` cannot be deleted or moved. Rename and move work only within one folder.
- `..`, absolute paths, and symlinks are refused, so nothing outside the folder is reachable. Symlinks are not listed.
- Indexing reads at most 2,000 files per folder and skips files over 256 KB, hidden files and folders, `node_modules`, and `target*` folders.

## Safety

- The vault root and the `rules/`, `skills/`, `memories/`, `global/`, `global/rules/`, `global/skills/`, and `folders/` folders cannot be deleted or moved.
- `/dav` sends no CORS headers, so other websites in your browser cannot read or change it.
- In read-only mode (`AX_WEB_READONLY=1`) and in share sessions, every write returns 403. Share sessions also require the share token, as the rest of the Command Center does.
- A single save is capped at 5 MB.

## Limits

- Changes made elsewhere (an agent through MCP, the Command Center) show up in Obsidian after you reopen the note. A WebDAV mount does not push change events. Changes you make in Obsidian reach ax as soon as Obsidian saves.
- When two editors save the same rule, the last save wins. Earlier versions stay in the rule's revision history.
- Obsidian may resolve an ambiguous bare name differently from ax (for example, a rule and a skill with the same name). Use a path link such as `[[skills/x]]` to be exact.
- A heading link (`[[pr#Steps]]`) still delivers the whole item to agents.
- Changes to a folder made outside the vault are imported at the next sync (start, **Sync now**, or a save through the vault), not immediately.
- By default `ax web` listens on `127.0.0.1`, so the vault is for the machine that runs it. Obsidian on a phone or another computer cannot open it.
- Code symbols are not pages. Use the [Command Center](/guides/command-center/) graph for code.

## Troubleshooting

| Problem | Fix |
|---|---|
| Connect fails or the drive is empty | Check that `ax web` is running and that `http://127.0.0.1:7070/ax/` opens in a browser. |
| Windows: "The network path was not found" | The WebClient service is not running. Start it with `sc start WebClient` (as administrator). |
| Windows: saving a large file fails | Windows limits WebDAV files to 50 MB by default. ax caps a single save at 5 MB anyway. |
| Obsidian shows an old version of a page | Reopen the note. A WebDAV mount does not push changes made elsewhere. |
| A save did not change the rule | Open `DRAFTS.md` to see why, fix the frontmatter, and save again. |
| The drive stays after `ax web` stops | Click **Disconnect** in Settings, or eject it (macOS), `net use Y: /delete` (Windows), `gio mount -u dav://127.0.0.1:7070/ax/` (Linux). |
