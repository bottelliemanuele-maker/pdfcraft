# PdfCraft in italiano: menu di macOS, «Salva con nome» e ricerca nella barra

Build locale di **PdfCraft 0.7.1** (ArtCraft, [storytold/pdfcraft](https://github.com/storytold/pdfcraft), MIT OR Apache-2.0) con tre modifiche di Emanuele Bottelli, tutte proposte al progetto. Creata il 13/10/2026.

PdfCraft ha già il catalogo italiano dell'interfaccia (`it.tsv`) e «Salva con nome…» nel menu interno (Menu ▸ File, ⇧⌘S). Le modifiche:

1. **Il menu dell'applicazione nella barra di macOS in italiano** (Informazioni su PdfCraft, Servizi, Nascondi PdfCraft, Nascondi altre, Mostra tutte, Esci da PdfCraft). AppKit lo costruisce in inglese per le app senza nib. Pull request [storytold/pdfcraft#1856](https://github.com/storytold/pdfcraft/pull/1856).
2. **Pulsante «Salva con nome» nella barra in alto**, accanto a Salva.
3. **Campo di ricerca nel documento fisso nella barra**, a sinistra di «Trova strumenti e comandi», al posto del riquadro flottante; Preferenze ▸ «Campo di ricerca nella barra degli strumenti» (acceso) per tornare al riquadro. Le modifiche 2 e 3 sono nella pull request [storytold/pdfcraft#1870](https://github.com/storytold/pdfcraft/pull/1870).

4. **Anteprime dei PDF nel Finder** (17/10/2026). Il tipo documento PDF di PdfCraft dichiarava una propria icona (`CFBundleTypeIconFile`): quando PdfCraft è l'app predefinita, il Finder mostra quell'icona per ogni PDF al posto della miniatura della prima pagina che Quick Look genera. Tolta la chiave dal modello `packaging/macos/Info.plist.in` (e, a mano, dall'app installata, con nuova firma ad hoc e `lsregister -f`): il Finder torna a mostrare le anteprime, come con Anteprima.
5. **La pagina si riadatta quando la finestra cambia dimensione** (17/10/2026). Con zoom «Adatta larghezza» o «Adatta pagina» PdfCraft si adattava già; dopo uno zoom manuale (⌘+, ⌘−, pinch) invece restava fisso. Ora, ridimensionando la finestra o il riquadro delle pagine, la vista torna all'ultimo adattamento scelto (larghezza o pagina). Preferenze ▸ «Riadatta la pagina quando la finestra cambia dimensione» (acceso) per tornare al comportamento di prima. Chiave `refit_on_resize` nelle impostazioni.

Le modifiche 4 e 5 sono nel ramo `bottelli-refit-finder` del fork (patch `0003`), riunite in `bottelli-italiano`.

## Che cosa c'è

| Cartella | Contenuto |
|---|---|
| `docs/bottelli/0001-i18n-…` | Il menu macOS (commit `863aaeb`, ramo `i18n-macos-app-menu`). |
| `docs/bottelli/0002-ui-…` | Salva con nome e ricerca nella barra (commit `4cbed92`, ramo `toolbar-save-as-and-find`). |
| `docs/bottelli/0003-…` | Anteprime PDF nel Finder e riadattamento al ridimensionamento (ramo `bottelli-refit-finder`). |
| `build/pdfcraft-0.7.1-italiano-6be3166-macos-aarch64.dmg` | L’app con tutte le modifiche 1-5 (ramo `bottelli-italiano`, commit `6be3166`; la 17590b9 è la build precedente), Apple Silicon, firma ad hoc (non notarizzata: va bene su questo Mac, non si distribuisce). Impronta in `SHA256SUMS`. |
| `anteprime/` | Il menu e la barra com'è resi dal vivo. |

## Dove sta su GitHub

- Fork: [bottelliemanuele-maker/pdfcraft](https://github.com/bottelliemanuele-maker/pdfcraft). Rami: `i18n-macos-app-menu`, `toolbar-save-as-and-find` (le due pull request) e `bottelli-italiano` (i due riuniti su `main`, la build installata).
- Quando le pull request vengono accolte, le release ufficiali hanno tutto e l'ArtCraft Launcher aggiorna `PdfCraft.app` da solo.

## Dove sta l'app

- **`/Applications/PdfCraft Italiano.app`**, 0.7.1, commit `6be3166` in `PdfCraftBuildCommit` di `Info.plist`. È l'unica PdfCraft installata e l'app predefinita per i PDF (`duti`).
- La 0.6.0 ufficiale è stata spostata il 13/10/2026 in `~/Desktop/claude/_da_eliminare/PdfCraft_0.6.0_ufficiale_2026-10-13.app` su richiesta di Emanuele. Se l'ArtCraft Launcher reinstalla `PdfCraft.app`, macOS sceglie per i PDF la copia con la versione più alta.

Sorgente e compilazione: `~/Library/Caches/PdfCraft_italiano/pdfcraft` (mai in iCloud).

## Su un altro Mac (stesse impostazioni)

1. Installare la build dal fork (`build/*.dmg` o rifarla come sotto) in `/Applications/PdfCraft Italiano.app`.
2. Renderla predefinita per i PDF: `duti -s ai.storyteller.pdfcraft com.adobe.pdf all`.
3. Se le anteprime dei PDF nel Finder non compaiono, l'app installata è una build senza la modifica 4: `plutil -remove "CFBundleDocumentTypes.0.CFBundleTypeIconFile" "/Applications/PdfCraft Italiano.app/Contents/Info.plist"`, poi `codesign --force --deep --sign - --options runtime "/Applications/PdfCraft Italiano.app"`, `lsregister -f` sull'app e `killall Finder`.
4. Le preferenze dell'app stanno in `~/Library/Application Support/PdfCraft/app.ron` (chiavi `default_zoom`, `find_in_toolbar`, `refit_on_resize`): non si sincronizzano da sole.

## Come rifare la build per una versione nuova

```bash
cd ~/Library/Caches/PdfCraft_italiano/pdfcraft
git fetch --tags origin && git checkout -b italiano-vX.Y.Z vX.Y.Z
git am docs/bottelli/000*.patch
cargo test -p pdfcraft-ui-egui --lib i18n && cargo test -p pdfcraft-ui-egui --test toolbar_find --test language_hook
MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release --locked -p pdfcraft -p pdfcraft-cli --target aarch64-apple-darwin
bash packaging/macos/package.sh --arch aarch64 --skip-build
ditto target/macos-package/PdfCraft.app "/Applications/PdfCraft Italiano.app"
```

Se una patch non si applica più, il sorgente è cambiato: va riletta (le due aggiungono righe in coda a `it.tsv` e `lt.tsv`, dove si toccano). Se `it.tsv` ufficiale contiene già `Quit {app}` e `Find text in the toolbar`, l'upstream ha accolto tutto e la build separata non serve più.

Sul Mac le prove di `commands.rs`, `document_links.rs`, `editing.rs` e `pickers.rs` che aspettano un'apertura in sfondo per un numero fisso di fotogrammi cadono anche su `main` pulito: non dipendono dalle patch.
