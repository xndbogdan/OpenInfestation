//! Paired successful marker allocation: 00437A90 and 00471AE0's inline tail.
//! Each marker owns distinct global+00 and local+0C links. The local header
//! has the exact +04/+10 sentinel shape of both 00435660 and 00471AE0.
//! Logical identities are never native pointers; unavailable backing is a
//! reached typed boundary, not an emulation of native NULL/fault/SEH.

use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerListKind {
    Global,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerLinkField {
    Next,
    Previous,
    Payload,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MarkerStorageError {
    #[error("paired marker local owner is unavailable")]
    OwnerUnavailable,
    #[error("paired marker allocation is unavailable")]
    MarkerUnavailable,
    #[error("{list:?} marker {field:?} backing is unavailable")]
    Unavailable {
        list: MarkerListKind,
        field: MarkerLinkField,
    },
    #[error("{list:?} marker {field:?} reached a NULL write receiver")]
    NullReceiver {
        list: MarkerListKind,
        field: MarkerLinkField,
    },
    #[error("paired marker local traversal reached a cycle outside the typed domain")]
    Cycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LocalId(pub(super) usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MarkerId(pub(super) usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Link {
    Null,
    Head,
    Tail,
    Marker(MarkerId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Links {
    pub(super) next: Option<Link>,
    pub(super) previous: Option<Link>,
    /// Outer None is unavailable; inner None is the actual NULL payload.
    pub(super) payload: Option<Option<MarkerId>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Header {
    pub(super) head: Links,
    pub(super) tail: Links,
}

impl Header {
    fn initialized() -> Self {
        Self {
            head: Links {
                next: Some(Link::Tail),
                previous: Some(Link::Null),
                payload: Some(None),
            },
            tail: Links {
                next: Some(Link::Null),
                previous: Some(Link::Head),
                payload: Some(None),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Marker {
    pub(super) global: Links,
    pub(super) local: Links,
    pub(super) owner: LocalId,
}

#[derive(Debug)]
pub(super) struct MarkerPool {
    pub(super) global: Header,
    pub(super) locals: Vec<Option<Header>>,
    pub(super) markers: Vec<Option<Marker>>,
}

impl MarkerPool {
    /// 0044FCF0 owns these two process sentinels once, independently of maps.
    pub(super) fn native_startup() -> Self {
        Self {
            global: Header::initialized(),
            locals: Vec::new(),
            markers: Vec::new(),
        }
    }

    /// Actual successful local allocation with the complete compatible header.
    pub(super) fn allocate_local(&mut self) -> LocalId {
        let id = LocalId(self.locals.len());
        self.locals.push(Some(Header::initialized()));
        id
    }

    fn links(&self, local: Option<LocalId>, link: Link) -> Result<&Links, MarkerStorageError> {
        let header = match local {
            None => &self.global,
            Some(id) => self
                .locals
                .get(id.0)
                .and_then(Option::as_ref)
                .ok_or(MarkerStorageError::OwnerUnavailable)?,
        };
        match link {
            Link::Head => Ok(&header.head),
            Link::Tail => Ok(&header.tail),
            Link::Marker(id) => {
                let marker = self
                    .markers
                    .get(id.0)
                    .and_then(Option::as_ref)
                    .ok_or(MarkerStorageError::MarkerUnavailable)?;
                if let Some(owner) = local {
                    if marker.owner != owner {
                        return Err(MarkerStorageError::OwnerUnavailable);
                    }
                    Ok(&marker.local)
                } else {
                    Ok(&marker.global)
                }
            }
            Link::Null => Err(MarkerStorageError::NullReceiver {
                list: kind(local),
                field: MarkerLinkField::Next,
            }),
        }
    }

    fn links_mut(
        &mut self,
        local: Option<LocalId>,
        link: Link,
    ) -> Result<&mut Links, MarkerStorageError> {
        match link {
            Link::Head | Link::Tail => {
                let header = match local {
                    None => &mut self.global,
                    Some(id) => self
                        .locals
                        .get_mut(id.0)
                        .and_then(Option::as_mut)
                        .ok_or(MarkerStorageError::OwnerUnavailable)?,
                };
                Ok(if link == Link::Head {
                    &mut header.head
                } else {
                    &mut header.tail
                })
            }
            Link::Marker(id) => {
                let marker = self
                    .markers
                    .get_mut(id.0)
                    .and_then(Option::as_mut)
                    .ok_or(MarkerStorageError::MarkerUnavailable)?;
                if let Some(owner) = local {
                    if marker.owner != owner {
                        return Err(MarkerStorageError::OwnerUnavailable);
                    }
                    Ok(&mut marker.local)
                } else {
                    Ok(&mut marker.global)
                }
            }
            Link::Null => Err(MarkerStorageError::NullReceiver {
                list: kind(local),
                field: MarkerLinkField::Next,
            }),
        }
    }

    fn read(
        &self,
        local: Option<LocalId>,
        link: Link,
        field: MarkerLinkField,
    ) -> Result<Link, MarkerStorageError> {
        let links = self.links(local, link)?;
        let value = match field {
            MarkerLinkField::Next => links.next,
            MarkerLinkField::Previous => links.previous,
            MarkerLinkField::Payload => unreachable!("payload is a separate typed field"),
        };
        value.ok_or(MarkerStorageError::Unavailable {
            list: kind(local),
            field,
        })
    }

    fn write(
        &mut self,
        local: Option<LocalId>,
        link: Link,
        field: MarkerLinkField,
        value: Link,
    ) -> Result<(), MarkerStorageError> {
        if link == Link::Null {
            return Err(MarkerStorageError::NullReceiver {
                list: kind(local),
                field,
            });
        }
        let links = self.links_mut(local, link)?;
        match field {
            MarkerLinkField::Next => links.next = Some(value),
            MarkerLinkField::Previous => links.previous = Some(value),
            MarkerLinkField::Payload => unreachable!("payload is a separate typed field"),
        }
        Ok(())
    }

    /// Successful allocation domain only. There is no synthetic NULL branch.
    /// 437AB7..C1 / 471B92..9C complete LOCAL before the fresh global predecessor.
    pub(super) fn register(&mut self, owner: LocalId) -> Result<MarkerId, MarkerStorageError> {
        let id = MarkerId(self.markers.len());
        let initialized = Links {
            next: None,
            previous: None,
            payload: Some(Some(id)),
        };
        self.markers.push(Some(Marker {
            global: initialized.clone(),
            local: initialized,
            owner,
        }));
        let local = Some(owner);
        let previous = self.read(local, Link::Tail, MarkerLinkField::Previous)?;
        let next = self.read(local, previous, MarkerLinkField::Next)?;
        self.write(local, Link::Marker(id), MarkerLinkField::Next, next)?;
        self.write(local, next, MarkerLinkField::Previous, Link::Marker(id))?;
        self.write(local, Link::Marker(id), MarkerLinkField::Previous, previous)?;
        self.write(local, previous, MarkerLinkField::Next, Link::Marker(id))?;
        // 437AC3 / 471B9E: do not capture this before the local insertion.
        let previous = self.read(None, Link::Tail, MarkerLinkField::Previous)?;
        let next = self.read(None, previous, MarkerLinkField::Next)?;
        self.write(None, Link::Marker(id), MarkerLinkField::Next, next)?;
        self.write(None, next, MarkerLinkField::Previous, Link::Marker(id))?;
        self.write(None, Link::Marker(id), MarkerLinkField::Previous, previous)?;
        self.write(None, previous, MarkerLinkField::Next, Link::Marker(id))?;
        Ok(id)
    }

    fn qualified(&self, owner: LocalId, link: Link) -> Result<Option<Link>, MarkerStorageError> {
        if link == Link::Null {
            return Ok(None);
        }
        Ok((self.read(Some(owner), link, MarkerLinkField::Next)? != Link::Null).then_some(link))
    }

    /// 00471BD0: capture successor, read payload, qualify captured successor,
    /// then test payload; each actual marker unlinks GLOBAL before LOCAL/free.
    pub(super) fn cleanup_local(&mut self, owner: LocalId) -> Result<(), MarkerStorageError> {
        let first = self.read(Some(owner), Link::Head, MarkerLinkField::Next)?;
        let mut current = self.qualified(owner, first)?;
        let mut visited = std::collections::BTreeSet::new();
        while let Some(link) = current {
            if self.read(Some(owner), link, MarkerLinkField::Previous)? == Link::Null {
                break;
            }
            let next = self.read(Some(owner), link, MarkerLinkField::Next)?;
            if next == Link::Null {
                break;
            }
            if let Link::Marker(id) = link {
                if !visited.insert(id.0) {
                    return Err(MarkerStorageError::Cycle);
                }
            }
            // 471BF1→471BF7→471BFA: retain this read order on typed stops.
            let payload =
                self.links(Some(owner), link)?
                    .payload
                    .ok_or(MarkerStorageError::Unavailable {
                        list: MarkerListKind::Local,
                        field: MarkerLinkField::Payload,
                    })?;
            current = self.qualified(owner, next)?;
            if let Some(id) = payload {
                for local in [None, Some(owner)] {
                    let next = self.read(local, Link::Marker(id), MarkerLinkField::Next)?;
                    let previous = self.read(local, Link::Marker(id), MarkerLinkField::Previous)?;
                    self.write(local, next, MarkerLinkField::Previous, previous)?;
                    // Fresh reads match 471C0F/12 and471C1F/22.
                    let previous = self.read(local, Link::Marker(id), MarkerLinkField::Previous)?;
                    let next = self.read(local, Link::Marker(id), MarkerLinkField::Next)?;
                    self.write(local, previous, MarkerLinkField::Next, next)?;
                }
                *self
                    .markers
                    .get_mut(id.0)
                    .ok_or(MarkerStorageError::MarkerUnavailable)? = None;
            }
        }
        Ok(())
    }

    pub(super) fn retire_local(&mut self, owner: LocalId) -> Result<(), MarkerStorageError> {
        *self
            .locals
            .get_mut(owner.0)
            .ok_or(MarkerStorageError::OwnerUnavailable)? = None;
        Ok(())
    }
}

fn kind(local: Option<LocalId>) -> MarkerListKind {
    if local.is_some() {
        MarkerListKind::Local
    } else {
        MarkerListKind::Global
    }
}
