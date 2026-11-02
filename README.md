# Google AI Limit Monitor (Windows Desktop Gadget)

[![Release](https://img.shields.io/badge/version-v1.0.0-blue.svg)](https://github.com/mowthow-works/google-ai-limit-monitor/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D6.svg)](https://www.microsoft.com/windows)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Google AI（Antigravity / Gemini）の利用枠である **「5-Hour Rolling Limit（5時間枠）」** および **「Weekly Limit（週間上限枠）」** の最新クォータ消費状況をデスクトップ上にリアルタイム表示する、Windows専用の常駐型デスクトップガジェットです。

<div align="center">
  <table>
    <tr>
      <td align="center">
        <b>⚡ Gemini Models</b><br />
        <img src="docs/screenshot_gemini.png" alt="Gemini Models" width="300" />
      </td>
      <td align="center">
        <b>💎 Gemini 4 Argon</b><br />
        <img src="docs/screenshot_argon.png" alt="Gemini 4 Argon" width="300" />
      </td>
    </tr>
    <tr>
      <td align="center">
        <b>🤖 Claude & GPT models</b><br />
        <img src="docs/screenshot_claude.png" alt="Claude and GPT models" width="300" />
      </td>
      <td align="center">
        <b>🟡 注意・警告時 (DEMOモード)</b><br />
        <img src="docs/screenshot_demo.png" alt="Demo Warning" width="300" />
      </td>
    </tr>
  </table>
</div>

---

## ✨ 主な特徴

- 🪟 **デスクトップ常駐 & 常に最前面 (Always On Top)**: 開発作業中も画面の隅に常に配置され、クォータ残量をいつでも一目で確認できます。
- 📑 **モデルグループ別のタブ切り替え対応**:
  - Antigravity の実際のクォータ管理体系に完全対応し、3つのグループをワンクリックで切り替えて正確にモニタリング可能：
    1. **Gemini**: 標準Gemini枠（5-Hour Limit: 89% & Weekly Limit: 94%）
    2. **Argon**: 最先端思考モデル専用枠（Weekly Limit: 95% ※5h制限なし）
    3. **Claude / GPT**: サードパーティモデル枠（5-Hour Limit: 100% & Weekly Limit: 100%）
- 🎨 **洗練されたフロストガラス & デュアルメーターUI**:
  - **3段階のカラーゲージ変化**:
    - 🟢 **80% 〜 100% (安全)**: エメラルドグリーン
    - 🟡 **30% 〜 79% (注意)**: アンバーオレンジ
    - 🔴 **0% 〜 29% (警告)**: ネオンレッド（枠枯渇寸前）
- ⚡ **超軽量設計 (Tauri 2.0 + Rust)**:
  - Windows標準の Edge WebView2 を利用するため、常駐時のメモリ消費量はわずか数十MB。
  - 完成した `.exe` は単一ポータブル実行ファイルとして動作（Node.js や Python などのランタイム不要）。
- 🔄 **完全自動検出 & ローカルRPC通信**:
  - ローカルで稼働している Language Server プロセスからポートと認証トークンを自動抽出し、専用RPC（`RetrieveUserQuotaSummary`）経由で最新データを10分ごとに自動取得（手動即時更新も可能）。
- 🖱️ **スムーズなタイトルバードラッグ**: ウィンドウ上部のタイトルバー領域を掴んで画面上の任意の位置へ手軽に配置可能。
- 🔒 **単一インスタンス制御 (Single Instance)**:
  - exeを誤って複数回実行してもガジェットが重複起動しません。既存のガジェットウィンドウが自動的に最小化解除され、最前面にフォーカスされます。
- 🧪 **デモ（エミュレーション）モード**:
  - タイトルバーのプランバッジ（`[PRO]`）をクリックすると、いつでも警告色（45% / 65%）表示をトグルで確認できます。

---

## 🛠️ システム要件

- **OS**: Windows 10 / 11 (64-bit)
- **ランタイム**: Microsoft Edge WebView2（Windows 10/11 に標準搭載）
- **対象環境**: Google Antigravity / Gemini 開発者環境（Language Server がローカルで稼働していること）

---

## 🚀 使い方

### 1. ダウンロードと起動
- [GitHub Releases](https://github.com/mowthow-works/google-ai-limit-monitor/releases/latest) から最新の `google-ai-limit-monitor.exe`（またはインストーラー）をダウンロードします。
- ダウンロードした `.exe` をダブルクリックするだけで、インストール不要のポータブルガジェットとして即座に起動します。
- （ローカルでソースからビルドした場合は、`src-tauri/target/release/google-ai-limit-monitor.exe` を直接起動できます）

### 2. ガジェットの操作
- **タブ切り替え [Gemini / Argon / Claude & GPT]**: クリックで各モデルグループのクォータ表示を瞬時に切り替えます。
- **移動**: ウィンドウ上部のタイトルバー（`Google AI Limits` 付近）をドラッグして画面上の自由な位置へ移動できます。
- **更新 [↻]**: ボタンをクリックすると即座に最新のクォータ情報を再取得します。
- **最小化 [_]**: タスクバーへ最小化します。
- **終了 [✕]**: ガジェットを終了します。
- **デモ切り替え [PRO]**: バッジをクリックすると警告表示（オレンジ／赤）のエミュレーション表示を切り替えられます。

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
