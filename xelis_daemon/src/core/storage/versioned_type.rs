use log::debug;
use xelis_common::{
    block::TopoHeight,
    serializer::{Reader, ReaderError, Serializer, Writer}
};

pub enum VersionedState {
    // Version is new
    New,
    // Version was fetched at topoheight
    FetchedAt(TopoHeight),
    // Version was fetched at topoheight but got updated
    Updated(TopoHeight),
}

impl VersionedState {
    pub fn is_new(&self) -> bool {
        matches!(self, Self::New)
    }

    pub fn is_fetched_at(&self) -> bool {
        matches!(self, Self::FetchedAt(_))
    }

    pub fn is_updated(&self) -> bool {
        matches!(self, Self::Updated(_))
    }

    pub fn should_be_stored(&self) -> bool {
        !self.is_fetched_at()
    }

    pub fn get_topoheight(&self) -> Option<TopoHeight> {
        match self {
            Self::FetchedAt(topoheight) | Self::Updated(topoheight) => Some(*topoheight),
            _ => None,
        }
    }

    pub fn mark_updated(&mut self) {
        match self {
            Self::FetchedAt(topoheight) => {
                *self = Self::Updated(*topoheight);
            },
            Self::Updated(_) => {},
            Self::New => {
                debug!("Cannot mark as updated a new version");
            },
        };
    }
}

// A versioned by topoheight data
pub struct Versioned<T: Serializer> {
    data: T,
    previous_topoheight: Option<TopoHeight>,
}

impl<T: Serializer> Versioned<T> {
    pub fn new(data: T, previous_topoheight: Option<TopoHeight>) -> Self {
        Self {
            data,
            previous_topoheight,
        }
    }

    pub fn get(&self) -> &T {
        &self.data
    }

    pub fn set(&mut self, data: T) {
        self.data = data;
    }

    pub fn get_previous_topoheight(&self) -> Option<TopoHeight> {
        self.previous_topoheight        
    }

    pub fn set_previous_topoheight(&mut self, previous_topoheight: Option<TopoHeight>) {
        self.previous_topoheight = previous_topoheight;
    }

    pub fn take(self) -> T {
        self.data
    }
}

impl<T: Serializer> Serializer for Versioned<T> {
    fn write(&self, writer: &mut Writer) {
        self.data.write(writer);
        if let Some(topo) = &self.previous_topoheight {
            topo.write(writer);
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, ReaderError> {
        let data = T::read(reader)?;
        let previous_topoheight = if reader.size() == 0 {
            None
        } else {
            Some(Reader::read(reader)?)
        };

        Ok(Self {
            data,
            previous_topoheight
        })
    }

    fn size(&self) -> usize {
        self.data.size() + if let Some(topoheight) = self.previous_topoheight { topoheight.size() } else { 0 }
    }
}