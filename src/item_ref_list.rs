use std::{ops::Deref, sync::Arc};

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use serde::de::DeserializeOwned;

use crate::{Error, ItemRef, Resolve};

/// A list of *references* to multiple `Lua` values.  
///
/// Useful to easily iterate over *lua table values* and get their [`ItemRef`] in rust.
#[derive(Debug, Clone)]
pub struct ItemRefList {
    /// The inner list, this needs to be wrapped to properly hold references and be sure they dont drop even if some reference still holds\
    /// and it must be wrapped in an [`RwLock`] since when we drop it we take `.refs`, thus clearing it and for that we need to mutate
    pub(crate) value: Arc<RwLock<LuaRefList>>,
}

/// Internal structure for [`ItemRefList`] to properly wrap them all in an [`Arc`] so references stay active as long as someone holds onto one
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LuaRefList {
    /// the source table which the items lay in
    pub(crate) source: ItemRef,
    /// all items inside `source`, referenced by their [`ItemRef`]
    pub(crate) refs: Vec<ItemRef>,
}

/// A reference to the internal list of all [`ItemRef`]'s from a [`ItemRefList`]
#[derive(Debug)]
pub struct RefList<'s> {
    pub(crate) guard: RwLockReadGuard<'s, LuaRefList>,
}

impl<'s> RefList<'s> {
    /// Returns a slice of all [`ItemRef`]'s
    #[inline]
    #[must_use]
    pub fn refs(&'s self) -> &'s [ItemRef] {
        &self.guard.refs
    }
}

impl ItemRefList {
    /// Creates a new [`ItemRefList`] with the internal wrapped value
    #[inline]
    #[must_use]
    pub(crate) fn new(source: ItemRef, refs: Vec<ItemRef>) -> Self {
        Self {
            value: Arc::new(RwLock::new(LuaRefList { source, refs })),
        }
    }

    /// Returns all keys from the source referenced table.  
    ///
    /// This sends a packet to the module to retrieve the keys.  
    ///
    /// ## Examples
    ///
    /// Take the following lua code:
    /// ```lua
    /// return { a = 1, b = 2, c = 3 }
    /// ```
    /// This would return a `Vec<String>` with the values `["a", "b", "c"]`;
    ///
    /// # Errors
    /// If the module executing the code fails or if the script can't be sent
    pub async fn keys<T>(&self) -> Result<Vec<T>, Error>
    where
        T: DeserializeOwned,
    {
        let source = &self.read().source;
        source.resolve().table_keys(source).await
    }

    /// Returns a list of all [`ItemRef`]'s
    ///
    /// ## Example
    ///
    /// How you would iterate over all references.\
    /// *Note that you must borrow the list. For consuming the references, see: [`take_list`](ItemRefList::take_list)*
    ///
    /// ```ignore
    /// let markers = timeline.store_list("self:GetMarkers()").await?;
    /// for marker in &markers.list() {
    ///     let _ = marker;
    /// }
    /// ```
    #[inline]
    #[must_use]
    pub fn list(&self) -> RefList<'_> {
        RefList { guard: self.read() }
    }

    /// Returns the number of references stored in the list
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.read().refs.len()
    }

    /// Returns a [`Vec`] of the inner [`ItemRef`]'s.  
    ///
    /// # Safety
    /// This function is `unsafe` since this can drastically hinder performance really easily.\
    /// *(There's a reason this list is always wrapped when normally returned)*\
    /// Unless you use [`ItemRefList::drop_all`] manually, each and every [`ItemRef`] will send a packet to drop itself in the module.\
    /// This can block your usual execution in the background, slowing down your application.\
    /// Only call this if you can take this performance hit or manually drop them in batches.
    #[inline]
    #[must_use]
    pub unsafe fn to_vec(&mut self) -> Vec<ItemRef> {
        std::mem::take(&mut self.write().refs)
    }

    /// Returns a read lock to the inner list value
    #[inline]
    pub(crate) fn read(&self) -> RwLockReadGuard<'_, LuaRefList> {
        self.value.read()
    }

    /// Returns a write lock to the inner list value
    #[inline]
    pub(crate) fn write(&self) -> RwLockWriteGuard<'_, LuaRefList> {
        self.value.write()
    }
}

// We have a custom DropMany packet for just this list since if we just took the drop impl
// from every reference in the stored list, we would send `N + 1` `DropItem` packets to the module.
// IN THE BACKGROUND, so it could send them while an actual .execute runs.
// And a single Resolve instance is locked with a Mutex during request since the module is single threaded anyway.
// and the Resolve instance uses the mutex to reuse buffers.
// so this would also lock that, so to prevent an insane amount of useless packets to clog the requests.
// we can instead send a list of ids for the module to remove from its handler at once, doing just a singular packet for the entire list
impl Drop for LuaRefList {
    fn drop(&mut self) {
        let mut ids = std::mem::take(&mut self.refs);

        // include the source ItemRef if we can in the DropMany packet
        if Arc::strong_count(&self.source.value) == 1 {
            let r = self.source.resolve();
            let item = std::mem::replace(&mut self.source, ItemRef::phantom(r));
            ids.push(item);
        }

        unsafe { ItemRefList::drop_all(self.source.resolve(), ids) };
    }
}

impl ItemRefList {
    /// Drops all provided [`ItemRef`]'s who *can* be dropped.  
    ///
    /// Each [`ItemRef`] must:
    /// - Not have been dropped already
    /// - Not have more than `1` *strong count* reference in it's internal [`Arc`]
    ///
    /// If an [`ItemRef`] satisfies this list, it will be marked as dropped to prevent it's own [`Drop`] from running.
    /// And then it will send a `DropMany` packet to drop all of the [`ItemRef`] in one packet.
    ///
    /// # Panics
    /// If an inner [`ItemRef`] `dropped` value is poisoned
    ///
    /// # Safety
    /// This can drop id's which may not be counted in their reference
    pub unsafe fn drop_all(resolve: Resolve, refs: Vec<ItemRef>) {
        let to_drop: Vec<u64> = refs
            .into_iter()
            // if theres more than 1 strong count, we dont drop it as the other reference will drop it later
            // and we dont want to manually drop something that has somehow already been dropped
            .filter(|x| Arc::strong_count(&x.value) == 1 && !x.is_dropped())
            .map(|x| {
                // after filtering, we mark the rest as dropped since were gonna batch drop them
                *x.value
                    .dropped
                    .write()
                    .expect("itemref.dropped was poisoned") = true;
                x.id()
            })
            .collect();

        tokio::spawn(async move {
            if let Err(e) = resolve.send_drop_items(&to_drop).await {
                eprintln!("{e:?}");
            }
        });
    }
}

impl Deref for RefList<'_> {
    type Target = [ItemRef];
    fn deref(&self) -> &Self::Target {
        self.refs()
    }
}

impl<'s> IntoIterator for &'s RefList<'s> {
    type Item = &'s ItemRef;
    type IntoIter = std::slice::Iter<'s, ItemRef>;

    fn into_iter(self) -> Self::IntoIter {
        self.refs().iter()
    }
}

impl PartialEq for ItemRefList {
    fn eq(&self, other: &Self) -> bool {
        *self.read() == *other.read()
    }
}
impl Eq for ItemRefList {}

impl PartialEq for RefList<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.refs() == other.refs()
    }
}
