pub mod occupation {
    use crate::competition::node::NodeHandle;
    use petgraph::graph::Node;
    use spacetimedb::{DbContext, Local, ReducerContext, table};
    struct RawServerOccupation {
        #[primary_key]
        pub(crate) server_id: u32,
        pub node_id: u32,
        pub node_variant: u8,
    }
    #[automatically_derived]
    impl spacetimedb::spacetimedb_lib::SpacetimeType for RawServerOccupation {
        fn make_type<S: spacetimedb::spacetimedb_lib::sats::typespace::TypespaceBuilder>(
            __typespace: &mut S,
        ) -> spacetimedb::spacetimedb_lib::sats::AlgebraicType {
            spacetimedb::spacetimedb_lib::sats::typespace::TypespaceBuilder::add(
                __typespace,
                core::any::TypeId::of::<RawServerOccupation>(),
                Some("RawServerOccupation"),
                |__typespace| {
                    spacetimedb::spacetimedb_lib::sats::AlgebraicType::product::<
                        [(
                            Option<&str>,
                            spacetimedb::spacetimedb_lib::sats::AlgebraicType,
                        ); 3usize],
                    >([
                        (
                            Some("server_id"),
                            <u32 as spacetimedb::spacetimedb_lib::SpacetimeType>::make_type(
                                __typespace,
                            ),
                        ),
                        (
                            Some("node_id"),
                            <u32 as spacetimedb::spacetimedb_lib::SpacetimeType>::make_type(
                                __typespace,
                            ),
                        ),
                        (
                            Some("node_variant"),
                            <u8 as spacetimedb::spacetimedb_lib::SpacetimeType>::make_type(
                                __typespace,
                            ),
                        ),
                    ])
                },
            )
        }
    }
    #[allow(non_camel_case_types)]
    #[allow(clippy::all)]
    const _: () = {
        impl<'de> spacetimedb::spacetimedb_lib::de::Deserialize<'de> for RawServerOccupation {
            fn deserialize<D: spacetimedb::spacetimedb_lib::de::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                deserializer.deserialize_product(__ProductVisitor {
                    _marker: std::marker::PhantomData::<fn() -> RawServerOccupation>,
                })
            }
        }
        struct __ProductVisitor {
            _marker: std::marker::PhantomData<fn() -> RawServerOccupation>,
        }
        impl<'de> spacetimedb::spacetimedb_lib::de::ProductVisitor<'de> for __ProductVisitor {
            type Output = RawServerOccupation;
            fn product_name(&self) -> Option<&str> {
                Some("RawServerOccupation")
            }
            fn product_len(&self) -> usize {
                3usize
            }
            fn visit_seq_product<A: spacetimedb::spacetimedb_lib::de::SeqProductAccess<'de>>(
                self,
                mut tup: A,
            ) -> Result<Self::Output, A::Error> {
                Ok(RawServerOccupation {
                    server_id: tup.next_element::<u32>()?.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::invalid_product_length(
                            0usize, &self,
                        )
                    })?,
                    node_id: tup.next_element::<u32>()?.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::invalid_product_length(
                            1usize, &self,
                        )
                    })?,
                    node_variant: tup.next_element::<u8>()?.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::invalid_product_length(
                            2usize, &self,
                        )
                    })?,
                })
            }
            fn visit_named_product<A: spacetimedb::spacetimedb_lib::de::NamedProductAccess<'de>>(
                self,
                mut __prod: A,
            ) -> Result<Self::Output, A::Error> {
                let mut server_id = None;
                let mut node_id = None;
                let mut node_variant = None;
                while let Some(__field) =
                    spacetimedb::spacetimedb_lib::de::NamedProductAccess::get_field_ident(
                        &mut __prod,
                        Self {
                            _marker: std::marker::PhantomData,
                        },
                    )?
                {
                    match __field {
                        __ProductFieldIdent::server_id => {
                            if server_id.is_some() {
                                return Err(
                                    spacetimedb::spacetimedb_lib::de::Error::duplicate_field(
                                        0usize,
                                        Some("server_id"),
                                        &self,
                                    ),
                                );
                            }
                            server_id = Some(
                                spacetimedb::spacetimedb_lib::de::NamedProductAccess::get_field_value::<
                                    u32,
                                >(&mut __prod)?,
                            );
                        }
                        __ProductFieldIdent::node_id => {
                            if node_id.is_some() {
                                return Err(
                                    spacetimedb::spacetimedb_lib::de::Error::duplicate_field(
                                        1usize,
                                        Some("node_id"),
                                        &self,
                                    ),
                                );
                            }
                            node_id = Some(
                                spacetimedb::spacetimedb_lib::de::NamedProductAccess::get_field_value::<
                                    u32,
                                >(&mut __prod)?,
                            );
                        }
                        __ProductFieldIdent::node_variant => {
                            if node_variant.is_some() {
                                return Err(
                                    spacetimedb::spacetimedb_lib::de::Error::duplicate_field(
                                        2usize,
                                        Some("node_variant"),
                                        &self,
                                    ),
                                );
                            }
                            node_variant = Some(
                                spacetimedb::spacetimedb_lib::de::NamedProductAccess::get_field_value::<
                                    u8,
                                >(&mut __prod)?,
                            );
                        }
                    }
                }
                Ok(RawServerOccupation {
                    server_id: server_id.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::missing_field(
                            0usize,
                            Some("server_id"),
                            &self,
                        )
                    })?,
                    node_id: node_id.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::missing_field(
                            1usize,
                            Some("node_id"),
                            &self,
                        )
                    })?,
                    node_variant: node_variant.ok_or_else(|| {
                        spacetimedb::spacetimedb_lib::de::Error::missing_field(
                            2usize,
                            Some("node_variant"),
                            &self,
                        )
                    })?,
                })
            }
        }
        impl<'de> spacetimedb::spacetimedb_lib::de::FieldNameVisitor<'de> for __ProductVisitor {
            type Output = __ProductFieldIdent;
            fn field_names(&self) -> impl '_ + Iterator<Item = Option<&str>> {
                ["server_id", "node_id", "node_variant"]
                    .into_iter()
                    .map(Some)
            }
            fn visit<__E: spacetimedb::spacetimedb_lib::de::Error>(
                self,
                name: &str,
            ) -> Result<Self::Output, __E> {
                match name {
                    "server_id" => Ok(__ProductFieldIdent::server_id),
                    "node_id" => Ok(__ProductFieldIdent::node_id),
                    "node_variant" => Ok(__ProductFieldIdent::node_variant),
                    _ => Err(spacetimedb::spacetimedb_lib::de::Error::unknown_field_name(
                        name, &self,
                    )),
                }
            }
            fn visit_seq(self, index: usize) -> Self::Output {
                match index {
                    0usize => __ProductFieldIdent::server_id,
                    1usize => __ProductFieldIdent::node_id,
                    2usize => __ProductFieldIdent::node_variant,
                    _ => ::core::panicking::panic("internal error: entered unreachable code"),
                }
            }
        }
        #[allow(non_camel_case_types)]
        enum __ProductFieldIdent {
            server_id,
            node_id,
            node_variant,
        }
    };
    impl spacetimedb::spacetimedb_lib::ser::Serialize for RawServerOccupation {
        fn serialize<S: spacetimedb::spacetimedb_lib::ser::Serializer>(
            &self,
            __serializer: S,
        ) -> Result<S::Ok, S::Error> {
            let mut __prod = __serializer.serialize_named_product(3usize)?;
            spacetimedb::spacetimedb_lib::ser::SerializeNamedProduct::serialize_element::<u32>(
                &mut __prod,
                Some("server_id"),
                &self.server_id,
            )?;
            spacetimedb::spacetimedb_lib::ser::SerializeNamedProduct::serialize_element::<u32>(
                &mut __prod,
                Some("node_id"),
                &self.node_id,
            )?;
            spacetimedb::spacetimedb_lib::ser::SerializeNamedProduct::serialize_element::<u8>(
                &mut __prod,
                Some("node_variant"),
                &self.node_variant,
            )?;
            spacetimedb::spacetimedb_lib::ser::SerializeNamedProduct::end(__prod)
        }
    }
    const _: () = {
        let _ = <u32 as spacetimedb::rt::TableColumn>::_ITEM;
        let _ = <u32 as spacetimedb::rt::TableColumn>::_ITEM;
        let _ = <u8 as spacetimedb::rt::TableColumn>::_ITEM;
    };
    #[allow(non_camel_case_types, dead_code)]
    trait tab_raw_server_occupation {
        #[allow(non_camel_case_types, dead_code)]
        fn tab_raw_server_occupation(&self) -> &tab_raw_server_occupation__TableHandle;
    }
    impl tab_raw_server_occupation for spacetimedb::Local {
        #[allow(non_camel_case_types, dead_code)]
        fn tab_raw_server_occupation(&self) -> &tab_raw_server_occupation__TableHandle {
            &tab_raw_server_occupation__TableHandle {}
        }
    }
    #[allow(non_camel_case_types, dead_code)]
    trait tab_raw_server_occupation__view {
        #[allow(non_camel_case_types, dead_code)]
        fn tab_raw_server_occupation(&self) -> &tab_raw_server_occupation__ViewHandle;
    }
    impl tab_raw_server_occupation__view for spacetimedb::LocalReadOnly {
        #[inline]
        fn tab_raw_server_occupation(&self) -> &tab_raw_server_occupation__ViewHandle {
            &tab_raw_server_occupation__ViewHandle {}
        }
    }
    #[allow(non_camel_case_types)]
    #[non_exhaustive]
    struct tab_raw_server_occupation__TableHandle {}
    #[allow(non_camel_case_types)]
    #[non_exhaustive]
    struct tab_raw_server_occupation__ViewHandle {}
    #[allow(non_camel_case_types, dead_code)]
    trait tab_raw_server_occupation__query {
        fn tab_raw_server_occupation(
            &self,
        ) -> spacetimedb::query_builder::Table<RawServerOccupation> {
            spacetimedb::query_builder::Table::new("tab_raw_server_occupation")
        }
    }
    impl tab_raw_server_occupation__query for spacetimedb::QueryBuilder {}
    #[allow(non_camel_case_types, dead_code)]
    pub struct RawServerOccupationCols {
        pub server_id: spacetimedb::query_builder::Col<RawServerOccupation, u32>,
        pub node_id: spacetimedb::query_builder::Col<RawServerOccupation, u32>,
        pub node_variant: spacetimedb::query_builder::Col<RawServerOccupation, u8>,
    }
    impl spacetimedb::query_builder::HasCols for RawServerOccupation {
        type Cols = RawServerOccupationCols;
        fn cols(_table_name: &'static str) -> Self::Cols {
            RawServerOccupationCols {
                server_id: spacetimedb::query_builder::Col::new(_table_name, "server_id"),
                node_id: spacetimedb::query_builder::Col::new(_table_name, "node_id"),
                node_variant: spacetimedb::query_builder::Col::new(_table_name, "node_variant"),
            }
        }
    }
    #[allow(non_camel_case_types, dead_code)]
    pub struct RawServerOccupationIxCols {
        pub server_id: spacetimedb::query_builder::IxCol<RawServerOccupation, u32>,
    }
    impl spacetimedb::query_builder::HasIxCols for RawServerOccupation {
        type IxCols = RawServerOccupationIxCols;
        fn ix_cols(_table_name: &'static str) -> Self::IxCols {
            RawServerOccupationIxCols {
                server_id: spacetimedb::query_builder::IxCol::new(_table_name, "server_id"),
            }
        }
    }
    impl spacetimedb::query_builder::CanBeLookupTable for RawServerOccupation {}
    const _: () = {
        impl tab_raw_server_occupation__TableHandle {
            ///Gets the [`UniqueColumn`][spacetimedb::UniqueColumn] for the [`server_id`][RawServerOccupation::server_id] column.
            pub(crate) fn server_id(
                &self,
            ) -> spacetimedb::UniqueColumn<
                tab_raw_server_occupation__TableHandle,
                u32,
                __indices::server_id,
            > {
                spacetimedb::UniqueColumn::__NEW
            }
            /**Gets the `node_handle` [`PointIndex`][spacetimedb::PointIndex] as defined on this table.

            This Hash index is defined on the following columns, in order:
            - [`node_variant`][RawServerOccupation#structfield.node_variant]: [`u8`]
            - [`node_id`][RawServerOccupation#structfield.node_id]: [`u32`]
            */
            fn node_handle(
                &self,
            ) -> spacetimedb::PointIndex<
                tab_raw_server_occupation__TableHandle,
                (u8, u32),
                __indices::node_handle,
            > {
                spacetimedb::PointIndex::__NEW
            }
        }
        impl tab_raw_server_occupation__ViewHandle {
            #[inline]
            pub fn count(&self) -> u64 {
                spacetimedb::table::count::<tab_raw_server_occupation__TableHandle>()
            }
            ///Gets the [`UniqueColumnReadOnly`][spacetimedb::UniqueColumnReadOnly] for the [`server_id`][RawServerOccupation::server_id] column.
            pub(crate) fn server_id(
                &self,
            ) -> spacetimedb::UniqueColumnReadOnly<
                tab_raw_server_occupation__TableHandle,
                u32,
                __indices::server_id,
            > {
                spacetimedb::UniqueColumnReadOnly::__NEW
            }
            /**Gets the `node_handle` [`PointIndexReadOnly`][spacetimedb::PointIndexReadOnly] as defined on this table.

            This Hash index is defined on the following columns, in order:
            - [`node_variant`][RawServerOccupation#structfield.node_variant]: [`u8`]
            - [`node_id`][RawServerOccupation#structfield.node_id]: [`u32`]
            */
            fn node_handle(
                &self,
            ) -> spacetimedb::PointIndexReadOnly<
                tab_raw_server_occupation__TableHandle,
                (u8, u32),
                __indices::node_handle,
            > {
                spacetimedb::PointIndexReadOnly::__NEW
            }
        }
        use spacetimedb::Serialize;
        impl spacetimedb::Table for tab_raw_server_occupation__TableHandle {
            type Row = RawServerOccupation;
            type UniqueConstraintViolation = spacetimedb::UniqueConstraintViolation;
            type AutoIncOverflow = ::core::convert::Infallible;
            fn integrate_generated_columns(
                __row: &mut RawServerOccupation,
                mut __generated_cols: &[u8],
            ) {
            }
        }
        impl spacetimedb::table::TableInternal for tab_raw_server_occupation__TableHandle {
            const TABLE_NAME: &'static str = "tab_raw_server_occupation";
            const UNIQUE_COLUMNS: &'static [u16] = &[0u16];
            const INDEXES: &'static [spacetimedb::table::IndexDesc<'static>] = &[
                spacetimedb::table::IndexDesc {
                    source_name: "tab_raw_server_occupation_server_id_idx_btree",
                    accessor_name: "server_id",
                    algo: spacetimedb::table::IndexAlgo::BTree { columns: &[0u16] },
                },
                spacetimedb::table::IndexDesc {
                    source_name: "tab_raw_server_occupation_node_variant_node_id_idx_hash",
                    accessor_name: "node_handle",
                    algo: spacetimedb::table::IndexAlgo::Hash {
                        columns: &[2u16, 1u16],
                    },
                },
            ];
            const PRIMARY_KEY: Option<u16> = Some(0u16);
            const SEQUENCES: &'static [u16] = &[];
            fn table_id() -> spacetimedb::TableId {
                static TABLE_ID: std::sync::OnceLock<spacetimedb::TableId> =
                    std::sync::OnceLock::new();
                *TABLE_ID.get_or_init(|| {
                    spacetimedb::table_id_from_name(
                        <Self as spacetimedb::table::TableInternal>::TABLE_NAME,
                    )
                })
            }
            fn get_default_col_values() -> Vec<spacetimedb::table::ColumnDefault> {
                [].to_vec()
            }
        }
        impl spacetimedb::rt::ExplicitNames for tab_raw_server_occupation__TableHandle {
            fn explicit_names() -> spacetimedb::spacetimedb_lib::ExplicitNames {
                let mut names = spacetimedb::spacetimedb_lib::ExplicitNames::default();
                names
            }
        }
        #[allow(non_camel_case_types)]
        mod __indices {
            #[allow(unused)]
            use super::*;
            pub(crate) struct server_id;
            impl spacetimedb::table::IndexIsRanged for server_id {}
            impl spacetimedb::table::Index for server_id {
                const NUM_COLS_INDEXED: usize = 1usize;
                fn index_id() -> spacetimedb::table::IndexId {
                    static INDEX_ID: std::sync::OnceLock<spacetimedb::table::IndexId> =
                        std::sync::OnceLock::new();
                    *INDEX_ID.get_or_init(|| {
                        spacetimedb::sys::index_id_from_name(
                            "tab_raw_server_occupation_server_id_idx_btree",
                        )
                        .unwrap()
                    })
                }
            }
            impl spacetimedb::table::Column for server_id {
                type Table = tab_raw_server_occupation__TableHandle;
                type ColType = u32;
                const COLUMN_NAME: &'static str = "server_id";
                fn get_field(row: &<Self::Table as spacetimedb::Table>::Row) -> &Self::ColType {
                    &row.server_id
                }
            }
            impl spacetimedb::table::PrimaryKey for server_id {}
            pub(super) struct node_handle;
            impl spacetimedb::table::IndexIsPointed for node_handle {}
            impl spacetimedb::table::Index for node_handle {
                const NUM_COLS_INDEXED: usize = 2usize;
                fn index_id() -> spacetimedb::table::IndexId {
                    static INDEX_ID: std::sync::OnceLock<spacetimedb::table::IndexId> =
                        std::sync::OnceLock::new();
                    *INDEX_ID.get_or_init(|| {
                        spacetimedb::sys::index_id_from_name(
                            "tab_raw_server_occupation_node_variant_node_id_idx_hash",
                        )
                        .unwrap()
                    })
                }
            }
        }
        #[unsafe(export_name = "__preinit__20_register_describer_tab_raw_server_occupation")]
        extern "C" fn __register_describer() {
            spacetimedb::rt::register_table::<tab_raw_server_occupation__TableHandle>()
        }
    };
    impl RawServerOccupation {
        fn new(node_handle: NodeHandle, server_id: u32) -> Self {
            let (node_variant, node_id) = node_handle.split();
            Self {
                server_id,
                node_id,
                node_variant,
            }
        }
    }
    pub(crate) trait TabRawServerOccupationRead {
        fn raw_server_is_occupied(&self, server_id: u32) -> bool;
        fn raw_server_occupation(&self, server_id: u32) -> Option<NodeHandle>;
        fn occupation_with_occupier(&self, node_handle: NodeHandle) -> Option<NodeHandle>;
    }
    pub(crate) trait TabRawServerOccupationWrite: TabRawServerOccupationRead {
        fn raw_server_occupation_add(
            &self,
            node_handle: NodeHandle,
            server_id: u32,
        ) -> Result<(), String>;
        fn raw_server_occupation_remove(&self, node_handle: NodeHandle) -> Result<(), String>;
    }
    impl<Db: DbContext> TabRawServerOccupationRead for Db {
        fn raw_server_is_occupied(&self, server_id: u32) -> bool {
            self.db_read_only()
                .tab_raw_server_occupation()
                .server_id()
                .find(server_id)
                .is_some()
        }
        fn raw_server_occupation(&self, server_id: u32) -> Option<NodeHandle> {
            self.db_read_only()
                .tab_raw_server_occupation()
                .server_id()
                .find(server_id)
                .map(|o| NodeHandle::combine(o.node_variant, o.node_id))
        }
        fn occupation_with_occupier(&self, node_handle: NodeHandle) -> Option<NodeHandle> {
            self.db_read_only()
                .tab_raw_server_occupation()
                .node_handle()
                .filter(node_handle.split())
                .next()
                .map(|o| NodeHandle::combine(o.node_variant, o.node_id))
        }
    }
    impl<Db: DbContext<DbView = Local>> TabRawServerOccupationWrite for Db {
        fn raw_server_occupation_add(
            &self,
            node_handle: NodeHandle,
            server_id: u32,
        ) -> Result<(), String> {
            self.db()
                .tab_raw_server_occupation()
                .server_id()
                .try_insert_or_update(RawServerOccupation::new(node_handle, server_id))?;
            Ok(())
        }
        fn raw_server_occupation_remove(&self, node_handle: NodeHandle) -> Result<(), String> {
            self.db()
                .tab_raw_server_occupation()
                .node_handle()
                .delete(node_handle.split());
            Ok(())
        }
    }
}
