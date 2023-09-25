pub use paste::paste;

#[macro_export]
macro_rules! widget {
(
    $name:ident<$state:ty,$actions:ty>,
    $widget_type:ty $(, $widget_args:expr)*;
    $($update_hook:expr)?,
    $($event_hook:expr)?
) => {
    pub struct $name {
        pub child: $widget_type,
    }

    impl $name {
        pub fn new() -> Self {
            Self {
                child: <$widget_type>::new($($widget_args),*),
            }
        }
    }

    impl Widget<$state,$actions> for $name {
        fn update(&mut self, state: $state) {
            $(
                $update_hook(self, state);
            )?
        }

        fn invalidate(&mut self) {
            self.child.invalidate();
        }

        fn event(&mut self, e: UiEvent) -> Option<$actions> {
            $(
                $event_hook(self, e)
            )?
        }

        fn render<D: Display>(&mut self, display: &mut D) {
            self.child.render(display);
        }
    }
};
}

#[macro_export]
macro_rules! widget_group {
(
    $name:ident<$state:ty,$actions:ty>,
    {$(
        $node_name:ident: $widget_type:ty $(, $widget_args:expr)*;
    )+},
    $($update_hook:expr)?,
    $($event_hook:expr)?
) => {
    pub struct $name {
        pub $($node_name: $widget_type,)+
    }

    impl Default for $name {
        fn default() -> Self {
            Self::new()
        }
    }

    impl $name {
        pub fn new() -> Self {
            Self {
                $(
                    $node_name: <$widget_type>::new($($widget_args),*),
                )+
            }
        }
    }

    impl Widget<$state,$actions> for $name {
        fn update(&mut self, state: $state) {
            $(
                $update_hook(self, state);
            )?
        }

        fn invalidate(&mut self) {
            $(
                self.$node_name.invalidate();
            )+
        }

        fn event(&mut self, event: UiEvent) -> Option<$actions> {
            $(
                $event_hook(self, event)
            )?
        }

        fn render(&mut self, display: &mut impl CharacterDisplay) {
            $(
                self.$node_name.render(display);
            )+
        }
    }
}
;}

#[macro_export]
macro_rules! widget_mux {
(
    $name:ident<$state:ty,$actions:ty>,
    $active:expr,
    {$(
        $node_name:ident: $widget_type:ty $(, $widget_args:expr)*;
    )+},
    $($update_hook:expr)?,
    $($event_hook:expr)?
) => {
    paste::paste! {
        #[derive(PartialEq, Eq, Clone, Copy, Debug)]
        pub enum [<$name:camel Node>] {
            $([<$node_name:camel>]),+
        }

        pub struct $name {
            active: [<$name:camel Node>],
            pub $($node_name: $widget_type,)+
        }

        impl $name {
            pub fn new() -> Self {
                let view = Self {
                    active: $active,
                    $(
                        $node_name: <$widget_type>::new($($widget_args),*),
                    )+
                };
                view
            }

            pub fn set_active(&mut self, node: [<$name:camel Node>]) {
                if self.active != node {
                    self.active = node;
                    self.invalidate();
                }
            }
        }

        impl Widget<$state,$actions> for $name {
            fn update(&mut self, state: $state) {
                $(
                    $update_hook(self, state);
                )?
            }

            fn invalidate(&mut self) {
                $(
                    self.$node_name.invalidate();
                )+
            }

            fn event(&mut self, event: UiEvent) -> Option<$actions> {
                $(
                    $event_hook(self, event)
                )?
            }

            fn render(&mut self, display: &mut impl CharacterDisplay) {
                $(
                    if self.active == [<$name:camel Node>]::[<$node_name:camel>] {
                        self.$node_name.render(display);
                    }
                )+
            }
        }
    }
}
;}
