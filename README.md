# local-runner-server
AtCoder Easy Test v2 用のローカルでテストできるサーバです。

## 使い方

### 1. 使いたい言語環境を用意する
`config.toml` に情報を登録します。

実行はコンパイルステップ、実行ステップからなります。

コンパイルステップではソースコードに対応するディレクトリが作成され、その中に指定した名前でソースコードが配置され、その他指定したファイルも `template` ディレクトリからコピーされます。その後 `compile_command` が実行され、正常終了すれば実行ステップへ進みます。

実行ステップではソースコードのあるディレクトリ内で `run_command` が実行されます。

```toml
# [compilers.適当なID]
[compilers.rust-1_70_0]
# C, Rust, JavaScript など。 AtCoder の言語選択欄にある言語名
language = "Rust"
# 環境選択のところに表示される
label = "Rust 1.70.0"
# ソースコードが置かれる、ソースコードディレクトリからの相対パス
source_filename = "src/main.rs" 
copy_files = [
    # `[src, dst]` の配列の形式で指定すると、 `template/src` からソースコードディレクトリ内の `dst` へコピーする
    ["rust-1_70_0/Cargo.toml", "Cargo.toml"],
    ["rust-1_70_0/rust-toolchain.toml", "rust-toolchain.toml"]
]
# コンパイルコマンド
compile_command = ["cargo", "build", "--release", "--quiet"]
# 実行コマンド
run_command = ["./target/release/main"]
```

### 2. サーバーを起動する
Rust が入っていなければ Rust をインストールする。

以下のコマンドを実行すると、 URL が表示されるのでコピーする。

```sh
$ cargo run
```

### 3. AtCoder Easy Test v2 で設定する
1. 適当な問題ページを開く。
2. 画面下部の「＾」を押し、右下にある「Setting」ボタンを押す。設定画面が出る。
3. `codeRunner.localRunnerURL` の左にあるテキストボックスに `2.` でコピーした URL を貼り付ける。
4. 設定画面を閉じ、問題ページをリロードする。
5. 使いたい言語を選択した状態で画面下部の「＾」を押し、 Environment のプルダウンを開くと `1.` で設定した環境が選択できる。