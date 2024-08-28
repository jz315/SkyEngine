use crate::reflect::{Type, TypeInfo};
use std::sync::Arc;
// 定义一个Component描述结构体，包括ID和大小
#[derive(Debug, Clone)]
pub struct Component {
    pub ty: Type,
    pub size: usize,
    pub align: usize,
}

impl Component {
    pub fn new(ty: Type) -> Component {
        Component {
            size: ty.size,
            align: ty.align,
            ty: ty,
        }
    }
}
