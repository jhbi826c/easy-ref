# easy-ref

用 Rust（Leptos + WebAssembly）写的文献管理工具，纯静态网页，部署在 GitHub Pages 上，数据可通过 GitHub 仓库在多设备间同步。

## 功能

- 粘贴 DOI、doi.org / ACM / Springer 等链接或 arXiv 编号（可一次粘贴多个）自动查询元数据并建立条目，数据来自 Crossref 和 DataCite
- 重复条目（DOI 相同，或标题与一作相同）不会重复添加
- 「全部文献」查看所有条目；可新建目录，一个条目可属于多个目录（拖拽或批量加入）
- 表格列：标题、作者、年份、期刊/会议、类型、引用 key、添加时间，点击表头排序
- 选中目录或部分条目导出 BibTeX，复制或下载 `.bib`
- 引用 key 格式为 `一作姓 + 年份 + 期刊/会议简称`，如 `He2016CVPR`、`Ren2017TPAMI`、`Vaswani2017arXiv`
- 导出前检查缺失字段和重复 key；特殊字符、重音字母、标题中的缩写大小写都会处理，常见 `.bst`（plain、IEEEtran、ACM 等）编译无警告
- 中文 / English 界面，浅色 / 深色 / 跟随系统

## 开发

需要 Nix（启用 flakes）。

```sh
direnv allow        # 或者 nix develop
trunk serve --open  # http://localhost:8080
cargo test
```

## 部署到 GitHub Pages

推送到 `main` 后，`.github/workflows/pages.yml` 会运行测试、构建并发布。第一次需要在仓库 **Settings → Pages → Build and deployment** 中把 Source 设为 **GitHub Actions**。

发布地址为 `https://<用户名>.github.io/<仓库名>/`。

## 同步

文献库保存为某个 GitHub 仓库中的一个 JSON 文件。建议单独建一个**私有**仓库存放数据：

1. 新建私有仓库，例如 `my-references`
2. 在 <https://github.com/settings/personal-access-tokens/new> 创建 fine-grained token：Repository access 只选这个仓库，Permissions → Contents 设为 **Read and write**
3. 在 easy-ref 的「设置 → GitHub 同步」中填写 token 和 `用户名/my-references`，点「保存并同步」

同步时会先拉取远端、合并再推送：同一条目以较新的修改为准，删除会同步到其他设备，不同设备添加的同一 DOI 会合并为一条。Token 只保存在当前浏览器的 localStorage 中。

## 已知限制

- IEEE Xplore 的 `ieeexplore.ieee.org/document/…` 链接里没有 DOI，需要复制页面上的 DOI
- NeurIPS、ICML、ICLR 等没有 DOI 的论文可以用 arXiv 编号添加，或手动新建后填写
- 两台设备同时修改同一个目录的成员时，以较晚的一次为准
