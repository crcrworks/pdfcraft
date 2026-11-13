# UPSTREAM — この fork の上流追従と差分

`crcrworks/pdfcraft` は [`storytold/pdfcraft`](https://github.com/storytold/pdfcraft) の fork。
bace（[`crcrworks/bace`](https://github.com/crcrworks/bace)）が LiveWindow に PdfCraft を
プロセス内で埋め込み、egui の frame を自分で回すために持っている。

目的は**改造を bace 側の資産にしないこと**。上流の取り込みを作り直しにしないための規則を
ここに置く。bace 側の決定は
[`docs/decisions/0031-craft-fork.md`](https://github.com/crcrworks/bace/blob/main/docs/decisions/0031-craft-fork.md)
（fork の共通規則）と `0046-pdfcraft-window.md`（この fork の差分そのもの）。

## ブランチ

| ブランチ | 役割 | 動かし方 |
| --- | --- | --- |
| `main` | 上流の mirror。追従専用で、ここには改造を積まない | `git fetch upstream && git merge --ff-only upstream/main`、`git push origin main` |
| `bace` | bace 用の改造を積む唯一のブランチ。bace の依存はここを指す | `git fetch upstream && git merge upstream/main`（merge commit で取り込む） |

- `bace` の起点にした上流の rev: `fd4d60ff4ae0e2f383ee57218ea554f37dd272d6`
  (2026-11-13, "CI: use the shared git-sources check (#7042)")
- `main` が mirror している上流の rev: 同上（`main` はまだ上流に遅れていない）
- `bace` が最後に取り込んだ上流の rev: 同上（起点から先の取り込みはまだ無い）

`main` は bace のビルドには使わない。bace が使うのは `bace` だけ。

## 差分の一覧

`bace` が上流と違うファイル。merge のたびに更新する。

| ファイル | 理由 | `// bace:` |
| --- | --- | --- |
| `UPSTREAM.md` | この文書。コードの差分ではない | — |
| `crates/ui-egui/src/host.rs` | **新しいモジュール。** eframe を持たないホスト（bace）の入口: PDF のバイト列を受け取って開く `open_pdf_bytes`、保存するバイト列を返す `save_front_bytes`、保存を記録する `mark_front_saved`、編集と再描画を区別する `document_revision`、文書の無い窓に置く `starter_pdf` | — |
| `crates/ui-egui/src/app.rs` | 1 フレーム分の `logic` / `ui` / `raw_input_hook` を固有メソッドにし、`eframe::App` はそれを呼ぶだけにする（`eframe::Frame` を受け取らない入口が要る） | 4 |
| `crates/ui-egui/src/lib.rs` | `host` モジュールを公開する | 1 |
| `crates/ui-egui/src/editing.rs` | `flatten_fill_sign_for_save` を `pub(crate)` に（`host::save_front_bytes` が Save と同じ後始末をする） | 1 |
| `crates/ui-egui/src/workflows/create_ui.rs` | `open_created_bytes` を `pub(crate)` に（`host::open_pdf_bytes` の入口） | 1 |

```bash
git diff fd4d60ff4ae0e2f383ee57218ea554f37dd272d6 bace --stat -- . ':!UPSTREAM.md'
git grep -c '// bace:'
```

## 既存ファイルへの変更はフックだけにする

上流が同じ関数を書き換えるたびに rebase が壊れるのを避けるため、改造は次の形に限る
（[`0031`](https://github.com/crcrworks/bace/blob/main/docs/decisions/0031-craft-fork.md) 決定 2・3）。

- bace 用の処理は新しいモジュールに置く（いまは `crates/ui-egui/src/host.rs` だけ）。
- 上流の関数を書き換えず、可視性を広げる・呼び出しを 1 行足すだけにする。
- 既存ファイルの差には `// bace: <理由>` を付ける。`git grep -c '// bace:'` で差の総量を数えられる状態を保つ。

`app.rs` の `logic` / `ui` だけは本体を移しているが、移した先は同じファイルの固有 impl で、
`eframe::App` の実装は 1 行の委譲だけになる（上流の処理そのものは変えていない）。

## ビルドと検証

bace は `branch = "bace"` でこのブランチを指し、正確な rev は bace の `Cargo.lock` に残す
（`cargo update -p pdfcraft-ui-egui` で進める）。fork 側の検証:

```bash
cargo fmt --all --check
cargo clippy -p pdfcraft-ui-egui --all-targets
cargo test -p pdfcraft-ui-egui
```

macOS の `platform::macos_association` / `platform::file_watch` の 6 テストは、この Mac の
ファイルシステムが拡張属性の保存を拒む（`os error 34`）ため落ちる。上流でも同じ環境で落ちる
ので、fork の判定には使わない（Linux の CI が上流の判定を持つ）。

## 上流へ返すかどうか

`host.rs` は「eframe を持たないホスト」の入口なので、上流にも意味がある。ただし bace 固有の
`starter_pdf`（bace の Works パネルが置く見本）を含むため、まずは fork に置く。
上流へ返すときは `starter_pdf` を bace 側へ移す。
