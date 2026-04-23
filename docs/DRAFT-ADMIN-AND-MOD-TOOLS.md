# sketch
```
/admin mods                                     open a moderator management dialog
    list
    add
    remove

/admin rooms                                    open a room management dialog
    *list all rooms
    *rename a room
    delete a room
    make a room private
    make a room public
    *ban user from a room
    *unban user from a room
    *kick a user from a room

/admin users                                    open a users management dialog
    *list of all online users
    *list of all users
    ban user by <ip|username|fingerprint> from server
    *temporary ban user by <ip|username|fingerprint> for <limited_duration> from server
    unban user from server
    delete user from server
    rename user 
    *rename user with user's consent
    *kick a user from server

/admin help                                     remind admin of what /admin commands are available

operations prefixed with * are availble to mods via:

/mod rooms                                      shows variant of rooms admin panel with admin-only features not rendered
/mod users                                      shows variant of users admin panel with admin-only features not rendered

```
# deferred
```
/admin roles                                    open a role management dialog
    create/edit role
        role optionally colorizes username
        role optionally adds emoji/text "flair" to username
    delete role
    assign role to user
    remove role from user
    view a user's roles
```

