use std::any::{type_name, TypeId};
use std::cell::RefCell;
use rustc_hash::FxHashMap;
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

thread_local! {
    static LOCAL_RUST_TYPES: RefCell<FxHashMap<TypeId, Type>> = RefCell::new(FxHashMap::default());
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
    name_to_type: FxHashMap<String, Type>,
    rust_type_to_type: FxHashMap<TypeId, Type>,
}

impl TypeMngr {
    fn new() -> Self {
        TypeMngr {
            name_to_type: FxHashMap::default(),
            rust_type_to_type: FxHashMap::default(),
        }
    }

    fn register(&mut self, name: &str, size: usize, align: usize) -> Type {
        if let Some(ty) = self.name_to_type.get(name) {
            return *ty;
        }

        let boxed_info = Box::new(TypeInfo::new(name, size, align));
        let static_info: &'static TypeInfo = Box::leak(boxed_info);

        let ty = Type::new(static_info);
        self.name_to_type.insert(name.to_string(), ty);

        ty
    }

    fn register_rust_type<T: 'static>(&mut self) -> Type {
        let rust_type_id = TypeId::of::<T>();
        if let Some(ty) = self.rust_type_to_type.get(&rust_type_id) {
            return *ty;
        }

        let name = type_name::<T>();
        if let Some(ty) = self.name_to_type.get(name).copied() {
            debug_assert_eq!(ty.size, core::mem::size_of::<T>());
            debug_assert_eq!(ty.align, core::mem::align_of::<T>());
            self.rust_type_to_type.insert(rust_type_id, ty);
            return ty;
        }

        let boxed_info = Box::new(TypeInfo::new(
            name,
            core::mem::size_of::<T>(),
            core::mem::align_of::<T>(),
        ));
        let static_info: &'static TypeInfo = Box::leak(boxed_info);

        let ty = Type::new(static_info);
        self.name_to_type.insert(name.to_string(), ty);
        self.rust_type_to_type.insert(rust_type_id, ty);

        ty
    }

    fn query_by_name(&self, name: &str) -> Option<Type> {
        self.name_to_type.get(name).cloned()
    }

    fn query_by_rust_type<T: 'static>(&self) -> Option<Type> {
        self.rust_type_to_type.get(&TypeId::of::<T>()).copied()
    }
}

// register a type
pub fn register(name: &str, size: usize, align: usize) -> Type {
    let mut mgr = TYPE_MNGR.write().unwrap();
    mgr.register(name, size, align)
}

pub fn register_rust_type<T: 'static>() -> Type {
    let rust_type_id = TypeId::of::<T>();

    if let Some(ty) = LOCAL_RUST_TYPES.with(|cache| cache.borrow().get(&rust_type_id).copied()) {
        return ty;
    }

    {
        let mgr = TYPE_MNGR.read().unwrap();
        if let Some(ty) = mgr.query_by_rust_type::<T>() {
            LOCAL_RUST_TYPES.with(|cache| {
                cache.borrow_mut().insert(rust_type_id, ty);
            });
            return ty;
        }
    }

    let mut mgr = TYPE_MNGR.write().unwrap();
    let ty = mgr.register_rust_type::<T>();
    LOCAL_RUST_TYPES.with(|cache| {
        cache.borrow_mut().insert(rust_type_id, ty);
    });
    ty
}

// query a type by name
pub fn query_by_name(name: &str) -> Option<Type> {
    let mgr = TYPE_MNGR.read().unwrap();
    mgr.query_by_name(name)
}

pub fn query_by_rust_type<T: 'static>() -> Option<Type> {
    let rust_type_id = TypeId::of::<T>();
    if let Some(ty) = LOCAL_RUST_TYPES.with(|cache| cache.borrow().get(&rust_type_id).copied()) {
        return Some(ty);
    }

    let mgr = TYPE_MNGR.read().unwrap();
    let ty = mgr.query_by_rust_type::<T>()?;
    LOCAL_RUST_TYPES.with(|cache| {
        cache.borrow_mut().insert(rust_type_id, ty);
    });
    Some(ty)
}
