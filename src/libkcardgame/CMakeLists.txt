add_library(kcardgame STATIC
    kabstractcarddeck.cpp
    kcard.cpp
    kcarddeck.cpp
    kcardpile.cpp
    kcardscene.cpp
    kcardtheme.cpp
    kcardthemewidget.cpp
    libkcardgame_debug.cpp
)

target_include_directories(kcardgame PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

target_link_libraries(kcardgame
    PUBLIC
        Qt6::Widgets
        kpatcompat
    PRIVATE
        Qt6::Svg
)
