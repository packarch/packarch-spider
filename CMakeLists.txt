cmake_minimum_required(VERSION 3.16 FATAL_ERROR)

project(packarch-spider VERSION 1.0.1 LANGUAGES C CXX)

# Packarch Spider: patience card games for Packarch, based on KDE's
# KPatience, built with Qt only (no KDE Frameworks / libkdegames / ECM).

set(QT_MIN_VERSION "6.5.0")

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_AUTOUIC ON)
# Translate the strings of .ui files through gettext (see src/compat/i18n.h).
set(CMAKE_AUTOUIC_OPTIONS -tr tr2i18n)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE RelWithDebInfo CACHE STRING "Build type" FORCE)
endif()

list(APPEND CMAKE_MODULE_PATH ${CMAKE_SOURCE_DIR}/cmake)

include(GNUInstallDirs)
include(FeatureSummary)

option(WITH_BH_SOLVER "Make use of https://github.com/shlomif/black-hole-solitaire for solving Golf" ON)
option(BUILD_TESTING "Build the autotests" OFF)
option(BUILD_TRANSLATIONS "Compile and install the translations (needs msgfmt from gettext)" ON)

find_package(Qt6 ${QT_MIN_VERSION} REQUIRED COMPONENTS
    Widgets
    Svg
    Xml
    Multimedia
)

find_package(FreecellSolver)
set_package_properties(FreecellSolver PROPERTIES TYPE REQUIRED
    PURPOSE "Solver used for hints and to detect won/lost games")

if(WITH_BH_SOLVER)
    find_package(BlackHoleSolver)
    set_package_properties(BlackHoleSolver PROPERTIES TYPE OPTIONAL
        PURPOSE "Solver for Golf")
    if(NOT BlackHoleSolver_FOUND)
        message(STATUS "black-hole-solver not found: building without the Golf solver")
        set(WITH_BH_SOLVER OFF)
    endif()
endif()

# The build tree mimics the install tree (bin/ + share/packarch-spider, share/locale)
# so that the game can be run straight from the build directory.
set(KPAT_BUILD_BINDIR ${CMAKE_BINARY_DIR}/${CMAKE_INSTALL_BINDIR})
set(KPAT_BUILD_DATADIR ${CMAKE_BINARY_DIR}/share/packarch-spider)
set(KPAT_BUILD_LOCALEDIR ${CMAKE_BINARY_DIR}/share/locale)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${KPAT_BUILD_BINDIR})

# Install destination (relative, so that "cmake --install --prefix" works) and
# the absolute paths compiled into the binary as a fallback lookup location.
set(KPAT_DATA_DESTINATION ${CMAKE_INSTALL_DATADIR}/packarch-spider)
set(KPAT_INSTALL_DATADIR ${CMAKE_INSTALL_FULL_DATADIR}/packarch-spider)
set(KPAT_INSTALL_LOCALEDIR ${CMAKE_INSTALL_FULL_LOCALEDIR})

add_compile_definitions(
    QT_DISABLE_DEPRECATED_UP_TO=0x060500
    QT_NO_KEYWORDS_FOR_SIGNALS_ONLY
)

# Copies a data directory to the build tree and installs it.
function(kpat_install_data source_dir dest_subdir)
    file(GLOB_RECURSE _files RELATIVE ${source_dir} ${source_dir}/*)
    list(FILTER _files EXCLUDE REGEX "(^|/)CMakeLists\\.txt$")
    foreach(_file ${_files})
        configure_file(${source_dir}/${_file} ${KPAT_BUILD_DATADIR}/${dest_subdir}/${_file} COPYONLY)
        get_filename_component(_dir ${_file} DIRECTORY)
        install(FILES ${source_dir}/${_file} DESTINATION ${KPAT_DATA_DESTINATION}/${dest_subdir}/${_dir})
    endforeach()
endfunction()

kpat_install_data(${CMAKE_SOURCE_DIR}/themes themes)
kpat_install_data(${CMAKE_SOURCE_DIR}/carddecks carddecks)
kpat_install_data(${CMAKE_SOURCE_DIR}/previews previews)
foreach(_sound card-pickup.ogg card-down.ogg)
    configure_file(${CMAKE_SOURCE_DIR}/sounds/${_sound} ${KPAT_BUILD_DATADIR}/sounds/${_sound} COPYONLY)
    install(FILES ${CMAKE_SOURCE_DIR}/sounds/${_sound} DESTINATION ${KPAT_DATA_DESTINATION}/sounds)
endforeach()

add_subdirectory(src)
add_subdirectory(icons)
add_subdirectory(mimetypes)

if(BUILD_TRANSLATIONS)
    add_subdirectory(po)
endif()

if(BUILD_TESTING)
    enable_testing()
    find_package(Qt6 ${QT_MIN_VERSION} REQUIRED COMPONENTS Test)
    add_subdirectory(autotests)
endif()

feature_summary(WHAT ALL FATAL_ON_MISSING_REQUIRED_PACKAGES)
