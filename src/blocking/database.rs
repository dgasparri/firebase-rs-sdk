use futures::executor::block_on;

impl Database {

    pub fn go_online(&self) -> DatabaseResult<()> {
        block_on(self.go_online_async())
    }

    pub fn go_offline(&self) -> DatabaseResult<()> {
        block_on(self.go_offline_async())
    }


}



impl DatabaseReference {

    pub fn get(&self) -> DatabaseResult<Value> {
        block_on(self.get_async())
    }

    pub fn push(&self) -> DatabaseResult<DatabaseReference> {
        block_on(self.push_async())
    }

    pub fn push_with_value<V>(&self, value: V) -> DatabaseResult<DatabaseReference>
    where
        V: Into<Value>,
    {
        block_on(self.push_with_value_async(value))
    }

    pub fn remove(&self) -> DatabaseResult<()> {
        block_on(self.remove_async())
    }


    pub fn set(&self, value: Value) -> DatabaseResult<()> {
        block_on(self.set_async(value))
    }

    pub fn set_priority<P>(&self, priority: P) -> DatabaseResult<()>
    where
        P: Into<Value>,
    {
        block_on(self.set_priority_async(priority))
    }


    pub fn set_with_priority<V, P>(&self, value: V, priority: P) -> DatabaseResult<()>
    where
        V: Into<Value>,
        P: Into<Value>,
    {
        block_on(self.set_with_priority_async(value, priority))
    }


    pub fn update(&self, updates: serde_json::Map<String, Value>) -> DatabaseResult<()> {
        block_on(self.update_async(updates))
    }


}

impl DatabaseQuery {

    pub fn get(&self) -> DatabaseResult<Value> {
        block_on(self.get_async())
    }

}