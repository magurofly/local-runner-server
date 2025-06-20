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