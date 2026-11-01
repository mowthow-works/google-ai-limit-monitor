# Google AI Limit Monitor (Windows Desktop Gadget)

Google AI（Antigravity / Gemini）の利用枠である **「5-Hour Rolling Limit（5時間枠）」** および **「Weekly Limit（週間上限枠）」** の最新クォータ消費状況をデスクトップ上にリアルタイム表示する、Windows専用の常駐型デスクトップガジェットです。

---

## ✨ 主な特徴

- 🪟 **デスクトップ常駐 & 常に最前面 (Always On Top)**: 開発作業中も画面の隅に常に配置され、クォータ残量をいつでも一目で確認できます。
- 🎨 **洗練されたフロストガラス & デュアルメーターUI**:
  - **5-Hour Limit（短期枠）**: Gemini Flash などのローリングリセット残量と次回リセットまでのカウントダウン
  - **Weekly Limit（週間枠）**: Gemini Argon などの週間リセット残量と次回リセットまでのカウントダウン
  - 残量に応じた動的なカラー変化（🟢 安全: 80%〜 / 🟡 注意: 30〜79% / 🔴 警告: 〜29%）
- ⚡ **超軽量設計 (Tauri 2.0 + Rust)**:
  - Windows標準の Edge WebView2 を利用するため、常駐時のメモリ消費量はわずか数十MB。
  - 完成した `.exe` は単一ポータブル実行ファイルとして動作（Node.js や Python などのランタイム不要）。
- 🔄 **完全自動検出 & ローカルRPC通信**:
  - ローカルで稼働している Language Server プロセスからポートと認証トークンを自動抽出し、直接 Connect-RPC（`127.0.0.1`）経由で最新データを15秒ごとに自動取得。
- 🖱️ **自由なドラッグ移動**: ウィンドウのどこを掴んでもスムーズに画面上の任意の位置へ配置可能。

---

## 🛠️ システム要件

- **OS**: Windows 10 / 11 (64-bit)
- **ランタイム**: Microsoft Edge WebView2（Windows 10/11 に標準搭載）
- **対象環境**: Google Antigravity / Gemini 開発者環境（Language Server がローカルで稼働していること）

---

## 🚀 使い方

### 1. 実行ファイルの起動
`src-tauri/target/release/google-ai-limit-monitor.exe`（または `debug` 版）をダブルクリックして起動します。

### 2. ガジェットの操作
- **移動**: ウィンドウ上をマウスでドラッグすると、画面上の自由な位置へ移動できます。
- **更新 [↻]**: ボタンをクリックすると即座に最新のクォータ情報を再取得します。
- **最小化 [_]**: タスクバーへ最小化します。
- **終了 [✕]**: ガジェットを終了します。

---

## 💻 開発・ビルド方法

### 開発環境の前提条件
- **Node.js**: v20以上
- **Rust**: 1.80以上 (MSVC ツールチェーン)
- **Visual Studio C++ Build Tools**

### 開発サーバー起動（ホットリロード）
```powershell
# 依存関係のインストール
npm install

# 開発モードで起動
npm run dev
# または
cargo run --manifest-path src-tauri/Cargo.toml
```

### リリースバイナリのビルド
```powershell
npm run build
# または
cargo build --release --manifest-path src-tauri/Cargo.toml
```
ビルド完了後、`src-tauri/target/release/google-ai-limit-monitor.exe` に単一実行ファイルが生成されます。

---

## 📜 ライセンス
MIT License
