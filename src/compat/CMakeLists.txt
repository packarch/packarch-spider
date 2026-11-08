# Small Qt-only replacements for the KDE Frameworks classes kpat used.
add_library(kpatcompat STATIC
    datapaths.cpp
    desktopfile.cpp
    i18n.cpp
    imagecache.cpp
    recentfilesaction.cpp
    selectaction.cpp
)

target_include_directories(kpatcompat PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

target_compile_definitions(kpatcompat PRIVATE
    KPAT_INSTALL_DATADIR="${KPAT_INSTALL_DATADIR}"
    KPAT_INSTALL_LOCALEDIR="${KPAT_INSTALL_LOCALEDIR}"
)

target_link_libraries(kpatcompat PUBLIC Qt6::Widgets)
