use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowsePageRequest {
    pub id: Uuid,
    pub offset: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrowseRowCountRequest(Uuid);

#[derive(Debug)]
pub struct BrowseLoadFailure {
    pub request: Option<BrowsePageRequest>,
    pub message: String,
}

#[derive(Debug, Default)]
pub(super) struct PageRequestTracker {
    latest: std::cell::Cell<Option<Uuid>>,
}

#[derive(Debug, Default)]
pub(super) struct RowCountRequestTracker {
    latest: std::cell::Cell<Option<Uuid>>,
}

impl RowCountRequestTracker {
    pub(super) fn begin(&self) -> BrowseRowCountRequest {
        let request = BrowseRowCountRequest(Uuid::new_v4());
        self.latest.set(Some(request.0));
        request
    }

    pub(super) fn accepts(&self, request: BrowseRowCountRequest) -> bool {
        self.latest.get() == Some(request.0)
    }
}

impl PageRequestTracker {
    pub(super) fn begin(&self, offset: u64) -> BrowsePageRequest {
        let request = BrowsePageRequest {
            id: Uuid::new_v4(),
            offset,
        };
        self.latest.set(Some(request.id));
        request
    }

    pub(super) fn accepts(&self, request: BrowsePageRequest, current_offset: u64) -> bool {
        self.latest.get() == Some(request.id) && request.offset == current_offset
    }
}
