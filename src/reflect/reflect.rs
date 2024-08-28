use std::collections::{HashMap, LinkedList};
use std::ops::Deref;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy)]
pub struct Type {
    info: &'static TypeInfo,
}

impl Type {
    fn new(info: &'static TypeInfo) -> Self {
        Type { info }
    }

    pub fn id(&self) -> usize {
        self.info as *const TypeInfo as usize
    }
}

// Implement Deref to allow `Type` to be treated like `&TypeInfo`
impl Deref for Type {
    type Target = TypeInfo;

    fn deref(&self) -> &Self::Target {
        self.info
    }
}

lazy_static::lazy_static! {
    static ref TYPE_MNGR: RwLock<TypeMngr> = RwLock::new(TypeMngr::new());
}

#[derive(Debug)]
pub struct TypeInfo {
    pub size: usize,
    pub align: usize,
    pub name: String,
}

impl TypeInfo {
    fn new(name: &str, size: usize, align: usize) -> Self {
        TypeInfo {
            name: name.to_string(),
            size,
            align,
        }
    }
}

struct TypeMngr {
    name_to_type: HashMap<String, Type>,
}

impl TypeMngr {
    fn new() -> Self {
        TypeMngr {
            name_to_type: HashMap::new(),
        }
    }

    fn register(&mut self, name: &str, size: usize, align: usize) -> Type {
        if let Some(ty) = self.name_to_type.get(name) {
            return ty.clone();
        }

        let boxed_info = Box::new(TypeInfo::new(name, size, align));
        let static_info: &'static TypeInfo = Box::leak(boxed_info);

        let ty = Type::new(static_info);
        self.name_to_type.insert(name.to_string(), ty.clone());

        ty
    }

    fn query_by_name(&self, name: &str) -> Option<Type> {
        self.name_to_type.get(name).cloned()
    }
}

// register a type
pub fn register(name: &str, size: usize, align: usize) -> Type {
    let mut mgr = TYPE_MNGR.write().unwrap();
    mgr.register(name, size, align)
}

// query a type by name
pub fn query_by_name(name: &str) -> Option<Type> {
    let mgr = TYPE_MNGR.read().unwrap();
    mgr.query_by_name(name)
}
