## v1.4.0 (2026-11-01)

### Feat

- drag files out to Finder on macOS with file promises
- drag promise writer that groups items and writes them without overwriting

### Fix

- keep a macOS drag that is still writing alive, and never clobber a file on drop

## v1.3.0 (2026-11-01)

### Feat

- attach the macOS DMG and binary to every release
- build a universal macOS DMG and binary

### Fix

- keep / separators in real paths below a macOS folder
- mark files opened from archives with quarantine on macOS

## v1.2.0 (2026-11-01)

### Feat

- a 3MF tile is a carousel of its plate renders and photos
- 3MF and LYS thumbnails, and mesh tiles that turn on hover

## v1.1.0 (2026-10-31)

### Feat

- tag several items at once
- select several items and drag or extract them together
- merge tags across a selection
- rubber-band hit testing
- selection model
- bulk tag commands
- drag and extract take targets from any folder

### Fix

- steady rubber band, locked items counted in a drag, no text selection
- bulk remove leaves a generated tag alone where another value overrides it

## v1.0.0 (2026-10-31)

### Feat

- release 1.0.0
- tag autocomplete in the search bar

## v0.5.0 (2026-10-31)

### Feat

- tag side panel
- tag dots and pill popover on tiles
- tag commands and tag search in the app backend
- tag terms in tree search
- schema v6 tag tables and tag store
- generated tags from support, scale, creator and edition
- tag model and tag query parsing

### Fix

- tag panel points a shared folder to its owner
- subfolders that share their parent's folder follow its tags
- a user-set tag masks the generated tag of its kind inherited from above
- copy_tags ignores untagged structural folders

## v0.4.0 (2026-10-31)

### Feat

- **ui**: view toggle button and search results as tiles
- show only configured file endings in both views

### Fix

- show unexpanded inner archives and drop stale search reveals

## v0.3.0 (2026-10-31)

### Feat

- **ui**: search bar and result list
- **ui**: search api types and result helpers
- **ui**: icon-only rescan button and a search toggle
- **app**: search command over the cached view trees
- **vtree**: name search with free, word and regexp modes
- **vtree**: keep each node's name with its case as label

### Fix

- **ui**: keep the grid's scroll behind search and select one hit

## v0.2.0 (2026-10-29)

### Refactor

- load parity spot checks from a local JSON file

## v0.1.0 (2026-10-29)

### Feat

- stop buttons, folder rescans, view-switch location, open files, STL folder pictures
- **ui**: drag tiles out, right-click Extract to…
- **app**: begin_drag and extract_to commands
- **drag**: stl-drag, OLE virtual-file data object with async copy
- **index**: locate export entries and write an export plan to disk
- **scan**: extraction session that keeps nested archives open
- **core**: export plan mirroring a view's tree, with Windows-safe names
- **thumb**: 256x256 JPEG thumbnails at quality 80
- **ui**: thumbnails and folder carousel
- **app**: thumbnail service, thumb-ready events and the thumb protocol
- **app**: carousel slots in listings and saved thumbnail selection
- **index**: thumbstore lookups and results
- **thumb**: priority job queue grouped by archive
- **thumb**: batch generation with one archive pass per batch
- **thumb**: STL/OBJ loading and CPU rasterizer
- **thumb**: cache names and image thumbnails
- **db**: schema v5 thumbs table
- **ui**: browse page with virtualized tile grid, breadcrumbs, view toggle, progress
- **ui**: api types, grid math, navigation history, tile subtitles
- **app**: background indexing with progress events, add folder, status
- **app**: cached views and list_folder
- **vtree**: walk by segments and per-node counts
- **index**: stlx parity against the prototype db; model hover from its files
- **index**: models and files views from the db; stlx index and tree
- **core**: files view
- **core**: models view
- **core**: virtual tree type and naming rules
- **index**: resolved tables (schema v4), per-collection resolve, overlapping roots
- **core**: resolver asset attachment and unit collapse
- **core**: resolver trees and model detection
- **core**: port name heuristics (variants, parts, images, parse_name)
- **scan**: flag password-protected archives as locked instead of scanning them (schema v3)
- **index**: incremental root scan and stlx dev cli
- **scan**: discover sources under a root
- **scan**: extract entry by nested chain
- **scan**: rar reader and nested archive listing
- **scan**: zip and 7z readers
- **core**: file kind and name helpers
- **app**: open index db on startup
- **index**: app dir, sqlite schema v1, settings

### Fix

- review findings for the usability pass
- **export**: read 7z and RAR levels in one pass for Extract to… and drags
- **app**: refuse drags and extractions while the index is updated
- **thumb**: stale jobs never write onto reused entry ids; passing I/O failures are retried; extraction panics fail the batch; one transaction per lookup
- **thumb**: read binary STLs by their size, so a 'solid' header doesn't send them to the ASCII parser
- **thumb**: macOS AppleDouble files (__MACOSX/, ._x.stl) get no thumbnail
- keep the index result when index-done beats the reply; commands off the main thread; add_folder holds the index claim
- durable re-resolve after an interrupted index, guard resolver panics, unique clash postfixes
- record parser panics as source errors instead of aborting
- **index**: detect renamed loose files with a name fingerprint (schema v2)
- **scan**: skip unreadable folders during discovery with a warning
- **scan**: stream 7z members and extract RAR-nested archives straight to disk
- **index**: retry sources that errored on the previous scan
