//! 左侧文件目录树的数据层：懒加载 + 展开/折叠 + 扁平化。

use std::fs;
use std::path::{Path, PathBuf};

/// 展开后的一行（渲染时直接遍历这个）。
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub expanded: bool,
}

struct Node {
    path: PathBuf,
    name: String,
    is_dir: bool,
    expanded: bool,
    /// `None` = 尚未读取过（懒加载）。
    children: Option<Vec<Node>>,
}

/// 简单的噪音过滤，按需扩展。
fn is_ignored(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "target" | "node_modules")
}

impl Node {
    fn new(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let is_dir = path.is_dir();
        Self { path, name, is_dir, expanded: false, children: None }
    }

    /// 读取直接子项，只会真正读一次。
    fn load(&mut self) {
        if self.children.is_some() || !self.is_dir {
            return;
        }
        let t = std::time::Instant::now();
        let n;
        let mut children = Vec::new();
        if let Ok(dir) = fs::read_dir(&self.path) {
            for entry in dir.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if is_ignored(&name) {
                    continue;
                }
                children.push(Node::new(entry.path()));
            }
        }
        // 目录在前，然后忽略大小写按名字排序
        children.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        n = children.len();
        eprintln!("load {:?}: {} entries in {:?}", self.path, n, t.elapsed());
        self.children = Some(children);
    }

    fn expand(&mut self) {
        self.expanded = true;
        self.load();
    }

    /// 递归展开到 `target`，返回是否命中。
    fn reveal(&mut self, target: &Path) -> bool {
        if self.path == target {
            return true;
        }
        if !target.starts_with(&self.path) {
            return false;
        }
        self.load();
        let Some(children) = self.children.as_mut() else {
            return false;
        };
        for child in children.iter_mut() {
            if child.reveal(target) {
                self.expanded = true;
                return true;
            }
        }
        false
    }

    fn toggle(&mut self, target: &Path) -> bool {
        if self.path == target {
            self.expanded = !self.expanded;
            if self.expanded {
                self.load();
            }
            return true;
        }
        if let Some(children) = self.children.as_mut() {
            for child in children.iter_mut() {
                if child.toggle(target) {
                    return true;
                }
            }
        }
        false
    }

    fn walk(&self, depth: usize, out: &mut Vec<Row>) {
        out.push(Row {
            path: self.path.clone(),
            name: self.name.clone(),
            depth,
            is_dir: self.is_dir,
            expanded: self.expanded,
        });
        if self.expanded {
            if let Some(children) = &self.children {
                for child in children {
                    child.walk(depth + 1, out);
                }
            }
        }
    }
}

pub struct FileTree {
    root_path: PathBuf,
    root: Node,
    selected: Option<PathBuf>,
}

impl FileTree {
    /// `root_path` 若是文件，自动取它的父目录。
    pub fn new(root_path: PathBuf) -> Self {
        let root_path = if root_path.is_file() {
            root_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(root_path)
        } else {
            root_path
        };

        let mut root = Node::new(root_path.clone());
        root.expand(); // 根目录默认展开

        Self { root_path, root, selected: None }
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn set_root(&mut self, root: PathBuf) {
        *self = Self::new(root);
    }

    pub fn selected(&self) -> Option<&Path> {
        self.selected.as_deref()
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        // 跳过根节点本身，直接从它的子项开始，depth = 0
        if let Some(children) = &self.root.children {
            for child in children {
                child.walk(0, &mut out);
            }
        }
        out
    }

    pub fn select(&mut self, path: &Path) {
        self.selected = Some(path.to_path_buf());
    }

    /// 展开目录 / 折叠目录。
    pub fn toggle(&mut self, path: &Path) {
        self.root.toggle(path);
    }

    /// 把某文件所在路径全部展开并选中它（打开外部文件时用）。
    pub fn reveal(&mut self, path: &Path) {
        self.root.reveal(path);
        self.selected = Some(path.to_path_buf());
    }

    /// 简单粗暴地重建整棵树，保留选中项。
    /// 文件保存 / 目录变化后调用。
    pub fn refresh(&mut self) {
        let selected = self.selected.clone();
        *self = Self::new(self.root_path.clone());
        if let Some(path) = selected {
            self.reveal(&path);
        }
    }
}
