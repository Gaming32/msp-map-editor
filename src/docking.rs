use imgui::*;

use imgui::{Direction, sys};

pub struct DockNode {
    id: u32,
}

impl DockNode {
    fn new(id: u32) -> Self {
        Self { id }
    }

    pub fn is_split(&self) -> bool {
        unsafe {
            let node = sys::igDockBuilderGetNode(self.id);
            if node.is_null() {
                false
            } else {
                sys::ImGuiDockNode_IsSplitNode(node)
            }
        }
    }

    /// Dock window into this dockspace
    #[doc(alias = "DockBuilder::DockWindow")]
    pub fn dock_window(&self, window: &str) {
        let window = ImString::from(window.to_string());
        unsafe { sys::igDockBuilderDockWindow(window.as_ptr(), self.id) }
    }

    #[doc(alias = "DockBuilder::SplitNode")]
    pub fn split<D, O>(&self, split_dir: Direction, size_ratio: f32, dir: D, opposite_dir: O)
    where
        D: FnOnce(DockNode),
        O: FnOnce(DockNode),
    {
        if self.is_split() {
            // Can't split an already split node (need to split the
            // node within)
            return;
        }

        let mut out_id_at_dir: sys::ImGuiID = 0;
        let mut out_id_at_opposite_dir: sys::ImGuiID = 0;
        unsafe {
            sys::igDockBuilderSplitNode(
                self.id,
                split_dir as i32,
                size_ratio,
                &mut out_id_at_dir,
                &mut out_id_at_opposite_dir,
            );
        }

        dir(DockNode::new(out_id_at_dir));
        opposite_dir(DockNode::new(out_id_at_opposite_dir));
    }
}

pub trait UiDocking {
    #[doc(alias = "DockSpaceOverViewport")]
    fn dockspace_over_viewport(&self) -> DockNode {
        unsafe {
            let id = sys::igDockSpaceOverViewport(
                sys::igGetMainViewport(),
                0,
                std::ptr::null::<sys::ImGuiWindowClass>(),
            );
            DockNode { id }
        }
    }
}

impl UiDocking for Ui {}
