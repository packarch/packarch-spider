foreach(_size 16 22 24 32 48 64 128 256)
    install(FILES ${_size}-apps-kpat.png
        DESTINATION ${CMAKE_INSTALL_DATADIR}/icons/hicolor/${_size}x${_size}/apps
        RENAME packarch-spider.png)
endforeach()
