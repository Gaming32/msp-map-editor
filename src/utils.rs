pub mod grid_as_vec_vec {
    use grid::Grid;
    use itertools::Itertools;
    use serde::de::Error;
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &Grid<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        struct SubSerialize<I>(I);
        impl<I> Serialize for SubSerialize<I>
        where
            I: ExactSizeIterator + Clone,
            I::Item: Serialize,
        {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
                for value in self.0.clone() {
                    seq.serialize_element(&value)?;
                }
                seq.end()
            }
        }

        let mut seq = serializer.serialize_seq(Some(value.rows()))?;
        for row in value.iter_rows() {
            seq.serialize_element(&SubSerialize(row))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<Grid<T>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let direct = Vec::<Vec<T>>::deserialize(deserializer)?;
        let columns = direct.first().map_or(0, |x| x.len());
        let flattened: Vec<_> = direct
            .into_iter()
            .map(|x| {
                if x.len() == columns {
                    Ok(x)
                } else {
                    Err(Error::invalid_length(
                        x.len(),
                        &columns.to_string().as_str(),
                    ))
                }
            })
            .flatten_ok()
            .try_collect()?;
        Ok(Grid::from_vec(flattened, columns))
    }
}
