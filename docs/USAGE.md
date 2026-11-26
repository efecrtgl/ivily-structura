# Using Ivily Structura

## 1. Open a project

*Open project folder* on the start screen, or from a terminal:

```powershell
"C:\Users\<you>\AppData\Local\Ivily Structura\ivily-structura.exe" C:\code\my-app            # open
"C:\Users\<you>\AppData\Local\Ivily Structura\ivily-structura.exe" C:\code\my-app --analyze  # open and analyse
```

Structura only reads the folder. Recent projects can be pinned or forgotten (forgetting removes
Structura's data about the project, never the project itself).

## 2. Analyse

Choose an **analysis profile** and press *Analyse*. The analysis runs in the background with a
progress bar and can be cancelled.

| Profile | Reads files | Graph | Typical use |
|---|---|---|---|
| Quick scan | no | no | Very large repositories, first look |
| Full architecture analysis | yes | yes | Default |
| Dependency analysis | yes | yes | Coupling and cycles only |
| Security-oriented review | yes | yes | Sensitive files, risky dependency specs; includes hidden files |
| Performance-oriented review | yes | yes | Large/complex files, hubs, duplicates |
| Documentation analysis | yes | no | README/docs coverage |

Duplicate a built-in profile in *Settings → Analysis profiles* to change exclusions, depth, size limits,
analyzers, rule categories, disabled rules and thresholds. Project-specific exclusions live in
*Settings → This project*.

The latest analysis is stored, so reopening a project shows results immediately.

## 3. Explore

- **Dashboard** — overview, languages, health summary, largest files and folders, hubs, snapshots,
  recent analyses, important modules. Most cards navigate into detail views.
- **Explorer** — the live tree (toggle hidden and git-ignored/excluded entries), a searchable and sortable
  file table, and the **file panel**: imports (with resolution method), files that use it, symbols,
  exports, complexity indicators, findings, git history, and your annotation.
- **Architecture graph** — files or folders (choose depth), force/hierarchy/concentric/circle/grid
  layouts, path highlight and prefix filter, cycles-only, external packages, tests, containment edges,
  focus on a file with direction (uses / used by) and depth, node limit, PNG/JSON export.
  Double-click a folder to expand it into files.
- **Health** — findings with severity, confidence, evidence, affected files and suggestions; the rule
  catalogue.
- **Dependencies** — external packages (used, undeclared, declared-but-unused), ecosystems and
  frameworks with evidence, cycles with concrete paths, unresolved imports.

## 4. Snapshots & comparison

Save a snapshot of any analysis (name + note). Open a snapshot to view its results, export/import it
as JSON, or compare: snapshot ↔ snapshot, snapshot ↔ current analysis, or this project ↔ another
analysed project. Comparisons show metric deltas (labelled exact/estimated), languages, added/removed
files, modules, packages, frameworks, dependency edges, new/resolved cycles and findings.

## 5. Project mapping

Your own architecture model, independent of the analysis and kept across re-analysis:

- **Layers** (top → bottom) with path patterns → *layer violation* findings on the next analysis.
- **Module groups**, **annotations** (label, note, important, entry point), **relations** you know about
  that static analysis cannot see, and **notes**.
- **Visual map**: folders placed into your layers, groups and relations.
- Import/export the mapping as JSON.

## 6. Reports

Markdown, JSON or CSV (files, findings, dependencies, packages, languages, directories). Choose
sections, title and list length, optionally include a snapshot comparison. Reports contain structure,
metrics and findings — never source code. Structura never writes reports into the project unless you
choose that location.

## 7. History & data

*History* lists analyses, snapshot operations, comparisons, exports and configuration changes (counts
only). Retention is configurable. *Settings → Data & backup* shows database health and creates or
restores backups (validated before replacing anything; a safety copy is kept).
