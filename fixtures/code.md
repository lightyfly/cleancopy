# 代码 Code

行内 `  padded code  `、`` `backticks` ``、`a < b && c > d` 与 `中文 English`。

## Fenced Rust

```rust
fn main() {
    let text = "中文  English";  

	println!("{text}");
    // trailing spaces below are intentional
    let padded = 1;    
}

```

## No language / leading and trailing blank lines

```

  leading two spaces
    leading four spaces
	leading tab
trailing two spaces  


```

## Tilde fence with metadata

~~~python title="example.py"
def greet(name):
    return f"你好, {name}"  
~~~

## Embedded fence and literal Markdown

````markdown
```rust
    literal fenced block
```
# not a heading
**not emphasis** <span>not HTML</span>
````

## Indented code

    first line
        second line

    final line  

## Empty fenced block

```text
```

最后一段 end。
