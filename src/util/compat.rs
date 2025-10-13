pub trait Compat<T> {
    fn delegate(&self) -> &T;
}

pub fn get_modular_instance<T>(service: &T) -> &T {
    service
}

pub fn get_compat_delegate<T, C>(service: &C) -> &T
where
    C: Compat<T>,
{
    service.delegate()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Wrapper<T> {
        inner: T,
    }

    impl<T> Compat<T> for Wrapper<T> {
        fn delegate(&self) -> &T {
            &self.inner
        }
    }

    #[test]
    fn returns_plain_instance() {
        let value = 10;
        assert_eq!(*get_modular_instance(&value), 10);
    }

    #[test]
    fn returns_delegate_for_wrapper() {
        let wrapped = Wrapper { inner: 42 };
        assert_eq!(*get_compat_delegate(&wrapped), 42);
    }
}
