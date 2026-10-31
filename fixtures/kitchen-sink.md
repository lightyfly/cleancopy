# CleanCopy 综合样例 Kitchen Sink

中英混排 Chinese + English，**加粗 bold**、*斜体 italic*、~~删除 deleted~~，以及 `cargo build --release`。

## 列表 Lists

- 第一项 apple
    - 第二层 nested
        - 第三层 leaf
    - 第二层 another
- 第二项 banana

7. 从七开始 start at seven
8. 接着是八 eight

- [ ] 待办 todo
- [x] 完成 done

### 引用 Quotes

> 第一行 quoted **重点**。
>
> 第二段包含 [文档 docs](https://example.com/docs)。
>
> > 内层引用 nested quote。

#### 表格 Tables

| 语言 Language | 分发 | 状态 |
| :--- | :---: | ---: |
| Rust | 单二进制 | **可用** |
| Go | 单二进制 | ~~旧版~~ |

##### 链接 Links、图片 Images 与 HTML

[CleanCopy 文档](https://example.com/docs "文档 title")，
[https://example.com](https://example.com)，
<https://example.com/auto>，www.example.org，hello@example.com。

![中文 alt / diagram](https://example.com/diagram.png "图片 title")

内联 <span class="label">保留 HTML 文字</span>，<strong>强调 emphasis</strong> 和 <br> 换行。

###### 代码 Code 与脚注 Footnotes

下面有两个空格的硬换行。  
这一行使用 soft
break，并带脚注[^note]。

```rust
fn main() {
    let message = "你好 world";

    println!("{message}");
}
```

---

正文结束，再次引用同一脚注[^note]。

[^note]: 脚注内容 footnote，含 **重点** 与 [来源](https://example.com/source)。
