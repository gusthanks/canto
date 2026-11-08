# \LibraryApi

All URIs are relative to *https://api.spotify.com/v1*

Method | HTTP request | Description
------------- | ------------- | -------------
[**change_playlist_details**](LibraryApi.md#change_playlist_details) | **PUT** /playlists/{playlist_id} | Change Playlist Details 
[**check_current_user_follows**](LibraryApi.md#check_current_user_follows) | **GET** /me/following/contains | Check If User Follows Artists or Users 
[**check_library_contains**](LibraryApi.md#check_library_contains) | **GET** /me/library/contains | Check User's Saved Items 
[**check_users_saved_albums**](LibraryApi.md#check_users_saved_albums) | **GET** /me/albums/contains | Check User's Saved Albums 
[**check_users_saved_audiobooks**](LibraryApi.md#check_users_saved_audiobooks) | **GET** /me/audiobooks/contains | Check User's Saved Audiobooks 
[**check_users_saved_episodes**](LibraryApi.md#check_users_saved_episodes) | **GET** /me/episodes/contains | Check User's Saved Episodes 
[**check_users_saved_shows**](LibraryApi.md#check_users_saved_shows) | **GET** /me/shows/contains | Check User's Saved Shows 
[**check_users_saved_tracks**](LibraryApi.md#check_users_saved_tracks) | **GET** /me/tracks/contains | Check User's Saved Tracks 
[**create_playlist**](LibraryApi.md#create_playlist) | **POST** /me/playlists | Create Playlist 
[**create_playlist_for_user**](LibraryApi.md#create_playlist_for_user) | **POST** /users/{user_id}/playlists | Create Playlist for user 
[**follow_artists_users**](LibraryApi.md#follow_artists_users) | **PUT** /me/following | Follow Artists or Users 
[**get_a_list_of_current_users_playlists**](LibraryApi.md#get_a_list_of_current_users_playlists) | **GET** /me/playlists | Get Current User's Playlists 
[**get_followed**](LibraryApi.md#get_followed) | **GET** /me/following | Get Followed Artists 
[**get_users_saved_albums**](LibraryApi.md#get_users_saved_albums) | **GET** /me/albums | Get User's Saved Albums 
[**get_users_saved_audiobooks**](LibraryApi.md#get_users_saved_audiobooks) | **GET** /me/audiobooks | Get User's Saved Audiobooks 
[**get_users_saved_episodes**](LibraryApi.md#get_users_saved_episodes) | **GET** /me/episodes | Get User's Saved Episodes 
[**get_users_saved_shows**](LibraryApi.md#get_users_saved_shows) | **GET** /me/shows | Get User's Saved Shows 
[**get_users_saved_tracks**](LibraryApi.md#get_users_saved_tracks) | **GET** /me/tracks | Get User's Saved Tracks 
[**get_users_top_artists_and_tracks**](LibraryApi.md#get_users_top_artists_and_tracks) | **GET** /me/top/{type} | Get User's Top Items 
[**remove_albums_user**](LibraryApi.md#remove_albums_user) | **DELETE** /me/albums | Remove Users' Saved Albums 
[**remove_audiobooks_user**](LibraryApi.md#remove_audiobooks_user) | **DELETE** /me/audiobooks | Remove User's Saved Audiobooks 
[**remove_episodes_user**](LibraryApi.md#remove_episodes_user) | **DELETE** /me/episodes | Remove User's Saved Episodes 
[**remove_library_items**](LibraryApi.md#remove_library_items) | **DELETE** /me/library | Remove Items from Library 
[**remove_shows_user**](LibraryApi.md#remove_shows_user) | **DELETE** /me/shows | Remove User's Saved Shows 
[**remove_tracks_user**](LibraryApi.md#remove_tracks_user) | **DELETE** /me/tracks | Remove User's Saved Tracks 
[**save_albums_user**](LibraryApi.md#save_albums_user) | **PUT** /me/albums | Save Albums for Current User 
[**save_audiobooks_user**](LibraryApi.md#save_audiobooks_user) | **PUT** /me/audiobooks | Save Audiobooks for Current User 
[**save_episodes_user**](LibraryApi.md#save_episodes_user) | **PUT** /me/episodes | Save Episodes for Current User 
[**save_library_items**](LibraryApi.md#save_library_items) | **PUT** /me/library | Save Items to Library 
[**save_shows_user**](LibraryApi.md#save_shows_user) | **PUT** /me/shows | Save Shows for Current User 
[**save_tracks_user**](LibraryApi.md#save_tracks_user) | **PUT** /me/tracks | Save Tracks for Current User 
[**unfollow_artists_users**](LibraryApi.md#unfollow_artists_users) | **DELETE** /me/following | Unfollow Artists or Users 



## change_playlist_details

> change_playlist_details(playlist_id, change_playlist_details_request)
Change Playlist Details 

Change a playlist's name and public/private state. (The user must, of course, own the playlist.) 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**playlist_id** | **String** |  | [required] |
**change_playlist_details_request** | Option<[**ChangePlaylistDetailsRequest**](ChangePlaylistDetailsRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_current_user_follows

> Vec<bool> check_current_user_follows(r#type, ids)
Check If User Follows Artists or Users 

Check to see if the current user is following one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_library_contains

> Vec<bool> check_library_contains(uris)
Check User's Saved Items 

Check if one or more items are already saved in the current user's library. Accepts Spotify URIs for tracks, albums, episodes, shows, audiobooks, artists, users, and playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**uris** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_albums

> Vec<bool> check_users_saved_albums(ids)
Check User's Saved Albums 

Check if one or more albums is already saved in the current Spotify user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_audiobooks

> Vec<bool> check_users_saved_audiobooks(ids)
Check User's Saved Audiobooks 

Check if one or more audiobooks are already saved in the current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_episodes

> Vec<bool> check_users_saved_episodes(ids)
Check User's Saved Episodes 

Check if one or more episodes is already saved in the current Spotify user's 'Your Episodes' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_shows

> Vec<bool> check_users_saved_shows(ids)
Check User's Saved Shows 

Check if one or more shows is already saved in the current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## check_users_saved_tracks

> Vec<bool> check_users_saved_tracks(ids)
Check User's Saved Tracks 

Check if one or more tracks is already saved in the current Spotify user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Check User's Saved Items](/documentation/web-api/reference/check-library-contains) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

**Vec<bool>**

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## create_playlist

> models::PlaylistObject create_playlist(create_playlist_request)
Create Playlist 

Create a playlist for the current Spotify user. (The playlist will be empty until you [add tracks](/documentation/web-api/reference/add-tracks-to-playlist).) Each user is generally limited to a maximum of 11000 playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**create_playlist_request** | Option<[**CreatePlaylistRequest**](CreatePlaylistRequest.md)> |  |  |

### Return type

[**models::PlaylistObject**](PlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## create_playlist_for_user

> models::PlaylistObject create_playlist_for_user(user_id, create_playlist_request)
Create Playlist for user 

**Deprecated**: Use [Create Playlist](/documentation/web-api/reference/create-playlist) instead.  Create a playlist for a Spotify user. (The playlist will be empty until you [add tracks](/documentation/web-api/reference/add-tracks-to-playlist).) Each user is generally limited to a maximum of 11000 playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**user_id** | **String** |  | [required] |
**create_playlist_request** | Option<[**CreatePlaylistRequest**](CreatePlaylistRequest.md)> |  |  |

### Return type

[**models::PlaylistObject**](PlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## follow_artists_users

> follow_artists_users(r#type, ids, follow_artists_users_request)
Follow Artists or Users 

Add the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**follow_artists_users_request** | Option<[**FollowArtistsUsersRequest**](FollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_a_list_of_current_users_playlists

> models::PagingPlaylistObject get_a_list_of_current_users_playlists(limit, offset)
Get Current User's Playlists 

Get a list of the playlists owned or followed by the current Spotify user. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingPlaylistObject**](PagingPlaylistObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_followed

> models::GetFollowed200Response get_followed(r#type, after, limit)
Get Followed Artists 

Get the current user's followed artists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**after** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]

### Return type

[**models::GetFollowed200Response**](get_followed_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_albums

> models::PagingSavedAlbumObject get_users_saved_albums(limit, offset, market)
Get User's Saved Albums 

Get a list of the albums saved in the current Spotify user's 'Your Music' library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]
**market** | Option<**String**> |  |  |

### Return type

[**models::PagingSavedAlbumObject**](PagingSavedAlbumObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_audiobooks

> models::PagingSimplifiedAudiobookObject get_users_saved_audiobooks(limit, offset)
Get User's Saved Audiobooks 

Get a list of the audiobooks saved in the current Spotify user's 'Your Music' library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSimplifiedAudiobookObject**](PagingSimplifiedAudiobookObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_episodes

> models::PagingSavedEpisodeObject get_users_saved_episodes(market, limit, offset)
Get User's Saved Episodes 

Get a list of the episodes saved in the current Spotify user's library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedEpisodeObject**](PagingSavedEpisodeObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_shows

> models::PagingSavedShowObject get_users_saved_shows(limit, offset)
Get User's Saved Shows 

Get a list of shows saved in the current Spotify user's library. Optional parameters can be used to limit the number of shows returned. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedShowObject**](PagingSavedShowObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_saved_tracks

> models::PagingSavedTrackObject get_users_saved_tracks(market, limit, offset)
Get User's Saved Tracks 

Get a list of the songs saved in the current Spotify user's 'Your Music' library. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**market** | Option<**String**> |  |  |
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::PagingSavedTrackObject**](PagingSavedTrackObject.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## get_users_top_artists_and_tracks

> models::GetUsersTopArtistsAndTracks200Response get_users_top_artists_and_tracks(r#type, time_range, limit, offset)
Get User's Top Items 

Get the current user's top artists or tracks based on calculated affinity. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**time_range** | Option<**String**> |  |  |[default to medium_term]
**limit** | Option<**i32**> |  |  |[default to 20]
**offset** | Option<**i32**> |  |  |[default to 0]

### Return type

[**models::GetUsersTopArtistsAndTracks200Response**](get_users_top_artists_and_tracks_200_response.md)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_albums_user

> remove_albums_user(ids, save_albums_user_request)
Remove Users' Saved Albums 

Remove one or more albums from the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_audiobooks_user

> remove_audiobooks_user(ids)
Remove User's Saved Audiobooks 

Remove one or more audiobooks from the Spotify user's library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_episodes_user

> remove_episodes_user(ids, remove_episodes_user_request)
Remove User's Saved Episodes 

Remove one or more episodes from the current user's library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**remove_episodes_user_request** | Option<[**RemoveEpisodesUserRequest**](RemoveEpisodesUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_library_items

> remove_library_items(uris)
Remove Items from Library 

Remove one or more items from the current user's library. Accepts Spotify URIs for tracks, albums, episodes, shows, audiobooks, users, and playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**uris** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_shows_user

> remove_shows_user(ids, market)
Remove User's Saved Shows 

Delete one or more shows from current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**market** | Option<**String**> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## remove_tracks_user

> remove_tracks_user(ids, save_albums_user_request)
Remove User's Saved Tracks 

Remove one or more tracks from the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_albums_user

> save_albums_user(ids, save_albums_user_request)
Save Albums for Current User 

Save one or more albums to the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_albums_user_request** | Option<[**SaveAlbumsUserRequest**](SaveAlbumsUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_audiobooks_user

> save_audiobooks_user(ids)
Save Audiobooks for Current User 

Save one or more audiobooks to the current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_episodes_user

> save_episodes_user(ids, save_episodes_user_request)
Save Episodes for Current User 

Save one or more episodes to the current user's library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |
**save_episodes_user_request** | Option<[**SaveEpisodesUserRequest**](SaveEpisodesUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_library_items

> save_library_items(uris)
Save Items to Library 

Add one or more items to the current user's library. Accepts Spotify URIs for tracks, albums, episodes, shows, audiobooks, users, and playlists. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**uris** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_shows_user

> save_shows_user(ids)
Save Shows for Current User 

Save one or more shows to current Spotify user's library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**ids** | **String** |  | [required] |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## save_tracks_user

> save_tracks_user(save_tracks_user_request)
Save Tracks for Current User 

Save one or more tracks to the current user's 'Your Music' library.  **Note:** This endpoint is deprecated. Use [Save Items to Library](/documentation/web-api/reference/save-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**save_tracks_user_request** | Option<[**SaveTracksUserRequest**](SaveTracksUserRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)


## unfollow_artists_users

> unfollow_artists_users(r#type, ids, unfollow_artists_users_request)
Unfollow Artists or Users 

Remove the current user as a follower of one or more artists or other Spotify users.  **Note:** This endpoint is deprecated. Use [Remove Items from Library](/documentation/web-api/reference/remove-library-items) instead. 

### Parameters


Name | Type | Description  | Required | Notes
------------- | ------------- | ------------- | ------------- | -------------
**r#type** | **String** |  | [required] |
**ids** | **String** |  | [required] |
**unfollow_artists_users_request** | Option<[**UnfollowArtistsUsersRequest**](UnfollowArtistsUsersRequest.md)> |  |  |

### Return type

 (empty response body)

### Authorization

[oauth_2_0](../README.md#oauth_2_0)

### HTTP request headers

- **Content-Type**: application/json
- **Accept**: application/json

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

