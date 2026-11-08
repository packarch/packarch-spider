add_subdirectory(compat)
add_subdirectory(libkcardgame)

configure_file(kpat_version.h.in ${CMAKE_CURRENT_BINARY_DIR}/kpat_version.h)

get_target_property(WITH_FCS_SOFT_SUSPEND FreecellSolver::FreecellSolver FREECELLSOLVER_HAS_SOFT_SUSPEND)
configure_file(patsolve-config.h.in ${CMAKE_CURRENT_BINARY_DIR}/patsolve-config.h)

# Game logic, shared by the application and the autotests.
add_library(kpatcore OBJECT
    dealer.cpp
    dealerinfo.cpp
    confirmdialog.cpp
    messagebox.cpp
    patpile.cpp
    pileutils.cpp
    renderer.cpp
    settings.cpp
    kpat_debug.cpp
    patsolve/abstract_fc_solve_solver.cpp
    patsolve/memory.cpp
    patsolve/patsolve.cpp

    bakersdozen.cpp
    patsolve/bakersdozensolver.cpp
    bizilizi.cpp
    patsolve/bizilizisolver.cpp
    castle.cpp
    patsolve/castlesolver.cpp
    clock.cpp
    patsolve/clocksolver.cpp
    fortyeight.cpp
    patsolve/fortyeightsolver.cpp
    freecell.cpp
    patsolve/freecellsolver.cpp
    golf.cpp
    grandf.cpp
    patsolve/grandfsolver.cpp
    normandy.cpp
    patsolve/normandysolver.cpp
    idiot.cpp
    patsolve/idiotsolver.cpp
    klondike.cpp
    patsolve/klondikesolver.cpp
    mod3.cpp
    patsolve/mod3solver.cpp
    simon.cpp
    patsolve/simonsolver.cpp
    spider.cpp
    patsolve/spidersolver2.cpp
    yukon.cpp
    patsolve/yukonsolver.cpp
)
if(WITH_BH_SOLVER)
    target_sources(kpatcore PRIVATE patsolve/golfsolver.cpp)
    target_link_libraries(kpatcore PUBLIC BlackHoleSolver::BlackHoleSolver)
endif()

target_include_directories(kpatcore PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
)
target_link_libraries(kpatcore PUBLIC
    kcardgame
    kpatcompat
    FreecellSolver::FreecellSolver
    Qt6::Widgets
    Qt6::Svg
    Qt6::Xml
)

# kpatcore is an OBJECT library: the game entries (DealerInfo objects)
# register themselves through static initializers, which a static archive
# would let the linker drop.
add_executable(packarch-spider
    main.cpp
    mainwindow.cpp
    appearancedialog.cpp
    gameselectionscene.cpp
    numbereddealdialog.cpp
    soundengine.cpp
    statisticsdialog.cpp
    statisticsdialog.ui
    view.cpp
    kpat.qrc
)
target_link_libraries(packarch-spider PRIVATE
    kpatcore
    Qt6::Multimedia
)

install(TARGETS packarch-spider RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR})
install(PROGRAMS org.packarch.Spider.desktop DESTINATION ${CMAKE_INSTALL_DATADIR}/applications)
install(FILES org.packarch.Spider.metainfo.xml DESTINATION ${CMAKE_INSTALL_DATADIR}/metainfo)
